use unicode_categories::UnicodeCategories;

#[allow(clippy::wrong_self_convention)]
pub(super) trait LexerCategories {
    fn is_lower_start(self) -> bool;
    fn is_upper_start(self) -> bool;
    fn is_name(self) -> bool;
    fn is_operator(self) -> bool;
}

impl LexerCategories for char {
    fn is_lower_start(self) -> bool {
        if self.is_ascii() {
            self.is_ascii_lowercase() || self == '_'
        } else {
            self.is_letter_lowercase()
        }
    }

    fn is_upper_start(self) -> bool {
        if self.is_ascii() { self.is_ascii_uppercase() } else { self.is_letter_uppercase() }
    }

    fn is_name(self) -> bool {
        if self.is_ascii() {
            self.is_ascii_alphanumeric() || self == '_' || self == '\''
        } else {
            self.is_alphanumeric()
        }
    }

    fn is_operator(self) -> bool {
        match self {
            // These are the only valid ASCII operators
            '~' | '!' | '#' | '$' | '%' | '&' | '*' | '+' | '.' | '/' | '<' | '=' | '>' | '?'
            | '@' | '\\' | '^' | '|' | '-' | ':' => true,
            _ => !self.is_ascii() && self.is_symbol(),
        }
    }
}
