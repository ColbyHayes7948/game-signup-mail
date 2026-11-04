// A second, smaller way to use what this repo already does.
// Kept separate from src/main.rs so the main path stays as short as it was.
// Reads INFRAI_API_KEY from the environment, same as the main example.
pub fn secondary_example() {
    match std::env::var("INFRAI_API_KEY") {
        Ok(_) => println!("key loaded; reuse the helper from the main example here"),
        Err(_) => println!("set INFRAI_API_KEY first"),
    }
}
