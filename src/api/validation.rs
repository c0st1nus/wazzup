//! Простые функции валидации для входных DTO.
//! Позволяет раннее отбрасывание некорректных данных.

use regex::Regex;

lazy_static::lazy_static! {
    static ref EMAIL_RE: Regex = Regex::new(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$").unwrap();
    static ref PHONE_RE: Regex = Regex::new(r"^[0-9+]{6,20}$").unwrap();
}

pub fn validate_email_opt(email: &str) -> bool {
    EMAIL_RE.is_match(email)
}
pub fn sanitize_phone(phone: &str) -> Option<String> {
    let digits: String = phone
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    if PHONE_RE.is_match(&digits) {
        Some(digits)
    } else {
        None
    }
}
pub fn ensure_max_len(value: &str, max: usize) -> bool {
    value.len() <= max
}
