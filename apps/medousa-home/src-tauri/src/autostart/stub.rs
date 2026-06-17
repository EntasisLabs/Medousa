pub fn install() -> Result<(), String> {
    Err(
        "Auto-start is not available on this platform yet. Start Medousa manually or use your OS task scheduler."
            .to_string(),
    )
}

pub fn remove() -> Result<(), String> {
    Ok(())
}
