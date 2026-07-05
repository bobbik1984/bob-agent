use std::sync::Mutex; struct CandleState { engine: Mutex<Option<()>>, } fn main() { let _ = CandleState { engine: Mutex::new(None) }; }
