#[cfg(not(target_os = "android"))]
pub use real_engine::*;

#[cfg(not(target_os = "android"))]
mod real_engine {
    use anyhow::Result;
    use candle_core::{Device, Tensor};
    use candle_core::quantized::gguf_file;
    use candle_transformers::models::quantized_llama::ModelWeights as Llama;
    use candle_transformers::models::quantized_qwen2::ModelWeights as Qwen2;
    use candle_transformers::models::quantized_qwen3::ModelWeights as Qwen3;
    use candle_transformers::generation::LogitsProcessor;
    use tokenizers::Tokenizer;
    use std::sync::Mutex;
    use tauri::State;
    use serde_json::json;

    pub enum QuantizedModel {
        Llama(Llama),
        Qwen2(Qwen2),
        Qwen3(Qwen3),
    }

    impl QuantizedModel {
        fn forward(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor> {
            match self {
                Self::Llama(m) => m.forward(input, index_pos).map_err(Into::into),
                Self::Qwen2(m) => m.forward(input, index_pos).map_err(Into::into),
                Self::Qwen3(m) => m.forward(input, index_pos).map_err(Into::into),
            }
        }
    }

    pub struct CandleEngine {
        model: QuantizedModel,
        tokenizer: Tokenizer,
        device: Device,
    }

    impl CandleEngine {
        pub fn new(model_path: &str, tokenizer_path: &str) -> Result<Self> {
            let device = Device::Cpu; 
            
            let mut file = std::fs::File::open(model_path)?;
            let content = gguf_file::Content::read(&mut file).map_err(|e| anyhow::anyhow!("GGUF Read error: {:?}", e))?;
            
            // Detect architecture from filename: qwen3 > qwen2 > llama
            let path_lower = model_path.to_lowercase();
            let model = if path_lower.contains("qwen3") || path_lower.contains("qwen-3") {
                QuantizedModel::Qwen3(Qwen3::from_gguf(content, &mut file, &device)?)
            } else if path_lower.contains("qwen") {
                QuantizedModel::Qwen2(Qwen2::from_gguf(content, &mut file, &device)?)
            } else {
                QuantizedModel::Llama(Llama::from_gguf(content, &mut file, &device)?)
            };
            
            let tokenizer_bytes = std::fs::read(tokenizer_path)
                .map_err(|e| anyhow::anyhow!("Failed to read tokenizer: {}", e))?;
            let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
                .map_err(|e| anyhow::anyhow!("Tokenizer load error: {}", e))?;
                
            Ok(Self {
                model,
                tokenizer,
                device,
            })
        }
        
        pub fn generate<F>(&mut self, prompt: &str, max_tokens: usize, temperature: f64, mut on_token: F) -> Result<()> 
        where
            F: FnMut(String) -> Result<()>
        {
            let tokens = self.tokenizer.encode(prompt, true).map_err(|e| anyhow::anyhow!("Encoding error: {}", e))?;
            let mut tokens = tokens.get_ids().to_vec();
            
            let mut logits_processor = LogitsProcessor::new(299792458, Some(temperature), None);
            
            let mut index_pos = 0;
            
            for _ in 0..max_tokens {
                let context_size = if index_pos == 0 { tokens.len() } else { 1 };
                let start_pos = tokens.len().saturating_sub(context_size);
                
                let input_tensor = Tensor::new(&tokens[start_pos..], &self.device)?.unsqueeze(0)?;
                
                let logits = self.model.forward(&input_tensor, index_pos)?;
                let logits = logits.squeeze(0)?.squeeze(0)?.to_dtype(candle_core::DType::F32)?;
                
                let next_token = logits_processor.sample(&logits)?;
                tokens.push(next_token);
                
                if let Some(text) = self.tokenizer.decode(&[next_token], true).ok() {
                    if !text.is_empty() {
                        on_token(text)?;
                    }
                }
                
                index_pos += context_size;
                
                // EOS tokens
                if next_token == 151643 || next_token == 151645 || next_token == 128001 || next_token == 128009 {
                    break;
                }
            }
            
            Ok(())
        }
    }

    pub struct CandleState {
        pub engine: Mutex<Option<CandleEngine>>,
        pub is_running: Mutex<bool>,
        pub current_model: Mutex<String>,
    }

    #[tauri::command]
    pub async fn start_offline_engine(model_path: String, state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
        if model_path.is_empty() {
            return Err("Model path is empty".to_string());
        }
        
        let mut resolved_path = std::path::PathBuf::from(&model_path);
        if !resolved_path.is_absolute() {
            let clean_path = model_path.strip_prefix("models\\").or_else(|| model_path.strip_prefix("models/")).unwrap_or(&model_path);
            let file_name = if clean_path.ends_with(".gguf") {
                clean_path.to_string()
            } else {
                format!("{}.gguf", clean_path)
            };
            let models_dir = crate::get_data_dir().join("models");
            resolved_path = models_dir.join(file_name);
        }
        
        let path = resolved_path.as_path();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path.parent().unwrap_or(std::path::Path::new(""));
        let tokenizer_path = parent.join(format!("{}_tokenizer.json", stem));

        match CandleEngine::new(&resolved_path.to_string_lossy(), &tokenizer_path.to_string_lossy()) {
            Ok(engine) => {
                *state.engine.lock().unwrap() = Some(engine);
                *state.is_running.lock().unwrap() = true;
                *state.current_model.lock().unwrap() = stem.to_string();
                Ok(json!({ "status": "started", "model": stem }))
            }
            Err(e) => {
                *state.is_running.lock().unwrap() = false;
                Err(format!("Failed to load engine (model={}, tok={}): {}", resolved_path.display(), tokenizer_path.display(), e))
            }
        }
    }

    #[tauri::command]
    pub async fn stop_offline_engine(state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
        *state.is_running.lock().unwrap() = false;
        *state.engine.lock().unwrap() = None;
        Ok(json!({ "status": "stopped" }))
    }

    #[tauri::command]
    pub async fn get_offline_engine_status(state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
        let running = *state.is_running.lock().unwrap();
        let current_model = state.current_model.lock().unwrap().clone();
        if running {
            Ok(json!({ "status": "running", "model": current_model }))
        } else {
            Ok(json!({ "status": "stopped", "model": current_model }))
        }
    }

    pub async fn run_native_inference(
        app: tauri::AppHandle,
        messages: Vec<serde_json::Value>,
        conv_id: String,
    ) -> serde_json::Value {
        use tauri::{Manager, Emitter};
        
        let state = app.state::<CandleState>();
        
        let running = *state.is_running.lock().unwrap();
        if !running {
            let config = crate::read_config();
            if let Some(path) = config.get("offlineModelPath").and_then(|v| v.as_str()) {
                if !path.is_empty() {
                    let app_clone = app.clone();
                    let conv_id_clone = conv_id.clone();
                    let _ = app_clone.emit("llm:chunk", json!({ "type": "thinking_replace", "content": "\n本地模型挂载中 0s...\n", "conv_id": &conv_id_clone }));
                    
                    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
                    
                    let app_clone_timer = app.clone();
                    let conv_id_clone_timer = conv_id.clone();
                    tokio::spawn(async move {
                        let mut secs = 0;
                        loop {
                            tokio::select! {
                                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                                    secs += 1;
                                    let _ = app_clone_timer.emit("llm:chunk", json!({ "type": "thinking_replace", "content": format!("\n本地模型挂载中 {}s...\n", secs), "conv_id": &conv_id_clone_timer }));
                                }
                                _ = &mut rx => {
                                    break;
                                }
                            }
                        }
                    });

                    let _ = start_offline_engine(path.to_string(), state.clone()).await;
                    let _ = tx.send(());
                }
            }
        }
        
        let running = *state.is_running.lock().unwrap();
        if !running {
            let app_clone = app.clone();
            let conv_id_clone = conv_id.clone();
            let _ = app_clone.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id_clone }));
            return json!({ "error": "Offline engine is not loaded. Please select a model in Settings first." });
        }
        
        let mut prompt = String::new();
        for m in &messages {
            if let (Some(role), Some(content)) = (m.get("role").and_then(|v| v.as_str()), m.get("content").and_then(|v| v.as_str())) {
                prompt.push_str(&format!("<|im_start|>{}\n{}<|im_end|>\n", role, content));
            }
        }
        prompt.push_str("<|im_start|>assistant\n");

        let app_clone = app.clone();
        let conv_id_clone = conv_id.clone();
        
        let res_json = tokio::task::spawn_blocking(move || {
            let state = app_clone.state::<CandleState>();
            let mut engine_guard = state.engine.lock().unwrap();
            
            if let Some(engine) = engine_guard.as_mut() {
                let mut full_response = String::new();
                
                // Emitting an initial thinking chunk so the UI reflects that inference has actually begun!
                let _ = app_clone.emit("llm:chunk", json!({ "type": "thinking", "content": "\n[本地离线引擎正在推理中，根据上下文长度可能需要 1~5 分钟 (CPU 运算)，请耐心等待]...\n", "conv_id": &conv_id_clone }));
                
                let res = engine.generate(&prompt, 1024, 0.7, |token| {
                    full_response.push_str(&token);
                    let _ = app_clone.emit("llm:chunk", json!({ "type": "text", "content": token, "conv_id": &conv_id_clone }));
                    Ok(())
                });
                
                let _ = app_clone.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id_clone }));
                
                match res {
                    Ok(_) => {
                        json!({
                            "role": "assistant",
                            "content": full_response,
                            "tool_calls": []
                        })
                    }
                    Err(e) => {
                        json!({ "error": format!("Candle Inference Error: {}", e) })
                    }
                }
            } else {
                let _ = app_clone.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id_clone }));
                json!({ "error": "Offline engine is not loaded" })
            }
        }).await.unwrap_or_else(|e| json!({ "error": format!("Tokio join error: {}", e) }));
        
        res_json
    }
}

#[cfg(target_os = "android")]
pub use dummy_engine::*;

#[cfg(target_os = "android")]
mod dummy_engine {
    use serde_json::{json, Value};
    use tauri::State;
    use std::sync::Mutex;
    
    pub struct CandleState {
        pub engine: Mutex<Option<()>>,
        pub is_running: Mutex<bool>,
        pub current_model: Mutex<String>,
    }
    
    #[tauri::command]
    pub async fn start_offline_engine(_model_path: String, _state: State<'_, CandleState>) -> Result<Value, String> {
        Err("Offline engine is not supported on Android".to_string())
    }
    
    #[tauri::command]
    pub async fn stop_offline_engine(_state: State<'_, CandleState>) -> Result<Value, String> {
        Ok(json!({ "status": "stopped" }))
    }
    
    #[tauri::command]
    pub async fn get_offline_engine_status(_state: State<'_, CandleState>) -> Result<Value, String> {
        Ok(json!({ "status": "stopped" }))
    }
    
    pub async fn run_native_inference(_app: tauri::AppHandle, _messages: Vec<Value>, _conv_id: String) -> Value {
        json!({ "error": "Offline engine is not supported on Android" })
    }
}

