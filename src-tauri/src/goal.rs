use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// 评估结果枚举
enum Verdict {
    Pass,
    Fail(String),
}

/// 解析 Clerk 模型返回的评估结果
fn parse_verdict(response: &Option<String>) -> Verdict {
    let text = response.as_deref().unwrap_or("");
    // 尝试寻找被包裹的 JSON，或者直接反序列化
    let start = text.find('{').unwrap_or(0);
    let end = text.rfind('}').map(|i| i + 1).unwrap_or(text.len());
    let json_str = &text[start..end];

    if let Ok(val) = serde_json::from_str::<Value>(json_str) {
        if let Some(verdict) = val.get("verdict").and_then(|v| v.as_str()) {
            if verdict == "PASS" {
                return Verdict::Pass;
            } else if let Some(feedback) = val.get("feedback").and_then(|v| v.as_str()) {
                return Verdict::Fail(feedback.to_string());
            }
        }
    }
    // Fallback 策略
    Verdict::Fail(text.to_string())
}

/// 抛出 Goal 进度事件到前端
fn emit_progress(app: &AppHandle, attempt: usize, max_rounds: usize, feedback: &Option<String>) {
    let payload = json!({
        "attempt": attempt,
        "max_rounds": max_rounds,
        "feedback": feedback.as_deref().unwrap_or(""),
    });
    let _ = app.emit("goal:progress", payload);
}

/// 持久化执行错误到 execution_errors 表 (目标 19)
fn persist_error(
    conv_id: Option<&str>,
    goal: &str,
    tool_name: &str,
    error_type: &str,
    error_msg: &str,
    context: Option<&str>,
) {
    let db_path = super::get_data_dir().join("bob.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let _ = crate::db::log_execution_error(
            &conn, conv_id, goal, tool_name, error_type, error_msg, context,
        );
    }
}

/// Goal Mode 外层闭环评估引擎
/// 执行内部 stream_internal，并用 call_clerk_oneshot 进行评估。若失败则重试。
/// 目标 19 增强：Layer 1 确定性断言 + 失败错误持久化
pub async fn execute_goal_loop(
    app: AppHandle,
    mut messages: Vec<Value>,
    conv_id: Option<String>,
) -> Value {
    let max_goal_rounds = 3;
    let mut attempt = 0;
    let mut feedback: Option<String> = None;
    let mut final_result = json!({});

    // 提取初始目标（最后一条 user 消息）
    let mut goal_text = String::new();
    if let Some(last) = messages.last() {
        if let Some(content) = last.get("content").and_then(|v| v.as_str()) {
            goal_text = content.to_string();
        }
    }

    loop {
        attempt += 1;
        if attempt > max_goal_rounds {
            // 安全护栏：超过最大重试次数
            let _ = app.emit("llm:chunk", json!({
                "type": "text",
                "content": format!("\n\n> ⚠️ [系统提示] 已达最大评估轮数 ({})，任务已强制终止。以下是最终执行结果。", max_goal_rounds)
            }));
            // 目标 19: 持久化预算耗尽错误
            persist_error(
                conv_id.as_deref(),
                &goal_text,
                "goal_loop",
                "budget_exhausted",
                &format!("{} 轮尝试均未达标", max_goal_rounds),
                None,
            );
            break;
        }

        // 如果有上一轮评估反馈，注入为新的 user 消息
        if let Some(ref fb) = feedback {
            messages.push(json!({
                "role": "user",
                "content": format!(
                    "上一次尝试未完全达成目标。评估器的反馈如下：\n{}\n\n请根据反馈修正并继续执行。",
                    fb
                )
            }));
        }

        // 抛出进度
        emit_progress(&app, attempt, max_goal_rounds, &feedback);

        // 内层：调用现有的 stream_internal（本身含有 5 轮工具循环）
        // agent_mode = "goal" 会在内部将工具调用预算提升至 50
        final_result = crate::llm::stream_internal(
            app.clone(),
            messages.clone(),
            conv_id.clone(),
            None,
            true, // global_file_access
            "goal".to_string(),
        )
        .await;

        let final_content = final_result
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // ══════════════════════════════════════════════════════════
        // 目标 19: Layer 1 — 确定性断言 (零 Token 消耗)
        // ══════════════════════════════════════════════════════════
        let tool_summary = final_result
            .get("tool_summary")
            .cloned()
            .unwrap_or(Value::Null);
        let assertion = crate::assertions::run_assertions(&tool_summary);

        if assertion.has_fatal() {
            // Fatal 断言失败：跳过昂贵的 Clerk 调用，直接重试
            log::warn!(
                "[Goal] Layer 1 assertion failed: {} fatal(s), {} warning(s)",
                assertion.failures.len(),
                assertion.warnings.len()
            );

            // 持久化断言失败
            for f in &assertion.failures {
                persist_error(
                    conv_id.as_deref(),
                    &goal_text,
                    "assertion",
                    &f.rule,
                    &f.description,
                    None,
                );
            }

            let feedback_text = assertion.format_feedback();
            let _ = app.emit(
                "llm:chunk",
                json!({
                    "type": "text",
                    "content": format!("\n\n> 🛡️ [Layer 1 断言] {}", feedback_text)
                }),
            );
            feedback = Some(feedback_text);
            continue; // 跳回循环顶部重试，不调用 Checker
        }

        // 如有 Warning，附加到 Checker 的评估上下文中
        let warning_ctx = if assertion.warnings.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n注意事项（来自确定性检查）：\n{}",
                assertion.warnings.join("\n")
            )
        };

        // ══════════════════════════════════════════════════════════
        // Layer 2: Checker — Clerk LLM 独立评估
        // ══════════════════════════════════════════════════════════
        let eval_prompt = format!(
            "你是一个严格的目标执行评估器（Evaluator）。\n\n\
            用户的原始目标是：「{}」\n\n\
            AI 助手的执行结果及回复是：\n{}\n{}\n\n\
            请判断目标是否已经完全达成。\n\
            - 如果已完全达成，输出 JSON: {{\"verdict\": \"PASS\"}}\n\
            - 如果未达成或部分达成，输出 JSON: {{\"verdict\": \"FAIL\", \"feedback\": \"具体的缺失项或错误，指示 AI 下一步该怎么做...\"}}\n\n\
            注意：默认立场为 FAIL，只有在确定目标完成时才输出 PASS。",
            goal_text, final_content, warning_ctx
        );

        let eval_result = crate::llm::call_clerk_oneshot(
            "你是严格的目标评估者。默认立场为 FAIL。必须输出 JSON 格式。",
            &eval_prompt,
            500,
        )
        .await;

        match parse_verdict(&eval_result) {
            Verdict::Pass => {
                let _ = app.emit(
                    "llm:chunk",
                    json!({
                        "type": "text",
                        "content": "\n\n> ✅ [目标评估] 评估器确认目标已达成。"
                    }),
                );
                break;
            }
            Verdict::Fail(fb) => {
                // 目标 19: 持久化 Checker 拒绝反馈
                persist_error(
                    conv_id.as_deref(),
                    &goal_text,
                    "checker",
                    "checker_rejection",
                    &fb,
                    Some(&final_content.chars().take(500).collect::<String>()),
                );
                feedback = Some(fb.clone());
                let _ = app.emit(
                    "llm:chunk",
                    json!({
                        "type": "text",
                        "content": format!("\n\n> 🔄 [目标评估] 评估器判定未达标。反馈：{}", fb)
                    }),
                );
                // 继续下一轮循环
            }
        }
    }

    final_result
}
