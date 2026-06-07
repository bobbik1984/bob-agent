fn main() { let s = "\u{FEFF}---"; println!("{}", s.strip_prefix("\u{FEFF}").unwrap_or(s) == "---"); }
