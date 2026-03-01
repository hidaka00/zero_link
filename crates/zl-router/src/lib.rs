#[derive(Debug)]
pub enum RouterError {
    InvalidTopic,
}

pub fn validate_topic(topic: &str) -> Result<(), RouterError> {
    if topic.is_empty() || topic.starts_with('/') || topic.ends_with('/') || topic.contains("//") {
        return Err(RouterError::InvalidTopic);
    }

    if !topic
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-'))
    {
        return Err(RouterError::InvalidTopic);
    }

    Ok(())
}
