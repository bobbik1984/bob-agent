use keyring::Entry;

fn main() {
    let provider = "qwen";
    let api_key = "sk-12345";
    
    let entry = Entry::new("com.bob.agent", provider).unwrap();
    
    println!("Storing...");
    match entry.set_password(api_key) {
        Ok(_) => println!("Store OK"),
        Err(e) => println!("Store Error: {}", e),
    }
    
    println!("Loading...");
    match entry.get_password() {
        Ok(pw) => println!("Load OK: {}", pw),
        Err(e) => println!("Load Error: {}", e),
    }
}
