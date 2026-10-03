use crate::FormatError;

pub(crate) struct Comment<'s> {
    pub(crate) text: &'s str,
    pub(crate) newline_before: bool,
    pub(crate) newline_after: bool,
    pub(crate) line: bool,
}

pub(crate) fn comments(mut source: &str) -> Result<Vec<Comment<'_>>, FormatError> {
    let mut comments = Vec::new();
    while !source.is_empty() {
        let trimmed = source.trim_start_matches(char::is_whitespace);
        let whitespace = &source[..source.len() - trimmed.len()];
        let newline_before = whitespace.contains(['\n', '\r']);
        source = trimmed;
        if source.is_empty() {
            break;
        }
        let line = source.starts_with("--");
        let length = if line {
            source.find(['\n', '\r']).unwrap_or(source.len())
        } else if source.starts_with("{-") {
            let mut depth = 1;
            let mut offset = 2;
            while depth > 0 {
                if offset == source.len() {
                    return Err(FormatError::UnterminatedComment);
                }
                if source[offset..].starts_with("{-") {
                    depth += 1;
                    offset += 2;
                } else if source[offset..].starts_with("-}") {
                    depth -= 1;
                    offset += 2;
                } else {
                    offset += source[offset..].chars().next().unwrap().len_utf8();
                }
            }
            offset
        } else {
            return Err(FormatError::SafetyCheck("unexpected text between tokens"));
        };
        let newline_after = source[length..]
            .chars()
            .take_while(|character| character.is_whitespace())
            .any(|character| matches!(character, '\n' | '\r'));
        comments.push(Comment { text: &source[..length], newline_before, newline_after, line });
        source = &source[length..];
    }
    Ok(comments)
}
