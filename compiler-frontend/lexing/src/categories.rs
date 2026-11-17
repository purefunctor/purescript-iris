use unicode_categories::UnicodeCategories;

const ASCII_NAME: [bool; 256] = {
    let mut table = [false; 256];
    let mut index = 0;
    while index < 128 {
        let byte = index as u8;
        table[index] = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'\'';
        index += 1;
    }
    table
};

pub(super) fn is_ascii_name(byte: u8) -> bool {
    ASCII_NAME[byte as usize]
}

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
        if self.is_ascii() { is_ascii_name(self as u8) } else { self.is_alphanumeric() }
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
