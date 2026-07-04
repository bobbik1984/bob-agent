use anyhow::Result;
use candle_core::{Device, Tensor};
use candle_core::quantized::ggml_file;
use candle_transformers::models::quantized_llama::ModelWeights as Llama;
use candle_transformers::generation::LogitsProcessor;
use tokenizers::Tokenizer;

pub struct CandleEngine {
    model: Llama,
    tokenizer: Tokenizer,
    device: Device,
}

impl CandleEngine {
    pub fn new(model_path: &str, tokenizer_path: Option<&str>) -> Result<Self> {
        let device = Device::Cpu; 
        
        let mut file = std::fs::File::open(model_path)?;
        let content = ggml_file::Content::read(&mut file, &device).map_err(|e| anyhow::anyhow!("GGML Read error: {:?}", e))?;
        let model = Llama::from_ggml(content, 1)?;
        
        let tokenizer_bytes = match tokenizer_path {
            Some(path) => std::fs::read(path)?,
            None => anyhow::bail!("Tokenizer path is required for initialization"),
        };
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
            
            // Assuming 151643 is the end-of-turn or EOS token for Qwen (could also be 151645)
            if next_token == 151643 || next_token == 151645 {
                break;
            }
        }
        
        Ok(())
    }
}

pub async fn run_native_inference(
    app: tauri::AppHandle,
    messages: Vec<serde_json::Value>,
    conv_id: String,
) -> serde_json::Value {
    use tauri::Emitter;
    use serde_json::json;

    let config = crate::read_config();
    let model_path = config.get("offlineModelPath").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    
    if model_path.is_empty() {
        let _ = app.emit("llm:chunk", json!({ "type": "text", "content": "Error: offlineModelPath not configured.", "conv_id": &conv_id }));
        let _ = app.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id }));
        return json!({ "error": "offlineModelPath not set" });
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
    
    let result = tokio::task::spawn_blocking(move || -> Result<String> {
        let mut engine = CandleEngine::new(&model_path, None)?;
        let mut full_response = String::new();
        
        engine.generate(&prompt, 1024, 0.7, |token| {
            full_response.push_str(&token);
            let _ = app_clone.emit("llm:chunk", json!({ "type": "text", "content": token, "conv_id": &conv_id_clone }));
            Ok(())
        })?;
        
        Ok(full_response)
    }).await;

    let _ = app.emit("llm:chunk", json!({ "type": "done", "content": "", "conv_id": &conv_id }));
    
    match result {
        Ok(Ok(content)) => {
            json!({
                "role": "assistant",
                "content": content,
                "tool_calls": []
            })
        }
        Ok(Err(e)) => {
            json!({ "error": format!("Candle Inference Error: {}", e) })
        }
        Err(e) => {
            json!({ "error": format!("Task Panic: {}", e) })
        }
    }
}

use std::sync::Mutex;
use tauri::State;

pub struct CandleState {
    pub engine: Mutex<Option<CandleEngine>>,
    pub is_running: Mutex<bool>,
}

#[tauri::command]
pub async fn start_offline_engine(model_path: String, state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
    if model_path.is_empty() {
        return Err("Model path is empty".to_string());
    }
    
    *state.is_running.lock().unwrap() = true;
    *state.engine.lock().unwrap() = None; // For now just mock it to return success
    
    Ok(serde_json::json!({ "status": "running", "message": "Engine started" }))
}

#[tauri::command]
pub async fn stop_offline_engine(state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
    *state.is_running.lock().unwrap() = false;
    *state.engine.lock().unwrap() = None;
    
    Ok(serde_json::json!({ "status": "stopped" }))
}

#[tauri::command]
pub async fn get_offline_engine_status(state: State<'_, CandleState>) -> Result<serde_json::Value, String> {
    let running = *state.is_running.lock().unwrap();
    if running {
        Ok(serde_json::json!({ "status": "running" }))
    } else {
        Ok(serde_json::json!({ "status": "stopped" }))
    }
}
