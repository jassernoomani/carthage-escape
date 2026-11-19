//! Kid nickname validation (PROJECT_SPEC.md §1.4): max 16 characters, no rude words,
//! and nothing that looks like personal data (emails, phone numbers, links).

pub const MAX_NICKNAME_CHARS: usize = 16;

/// Matched as substrings of the normalized nickname. Deliberately avoids fragments that
/// appear inside real names or words: "ass" (Hassan), "cul" (Hercule), "nique" (Dominique),
/// "nazi" (Nazih), "pute" (Computer), "rape" (Grape), "cock" (Peacock), "fuk" (Fukuda).
/// Entries of 2 characters or fewer only match whole words.
const BLOCKLIST: &[&str] = &[
    // English
    "fuck", "shit", "bitch", "cunt", "dick", "pussy", "asshole", "bastard", "whore", "slut",
    "nigger", "nigga", "faggot", "retard", "porn", "penis", "vagina", "boob", "hitler", "sexy",
    // French
    "merde", "putain", "connard", "connasse", "salope", "encule", "batard", "couille", "pd",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NicknameError {
    #[error("nickname is empty")]
    Empty,
    #[error("nickname is longer than {MAX_NICKNAME_CHARS} characters")]
    TooLong,
    #[error("nickname contains characters that are not allowed")]
    BadCharacters,
    #[error("nickname looks like personal information")]
    PersonalInfo,
    #[error("nickname is not allowed")]
    Blocked,
}

/// Returns the cleaned-up nickname (trimmed, inner whitespace collapsed) or why it was rejected.
pub fn validate(raw: &str) -> Result<String, NicknameError> {
    let nickname = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if nickname.is_empty() {
        return Err(NicknameError::Empty);
    }
    if nickname.chars().count() > MAX_NICKNAME_CHARS {
        return Err(NicknameError::TooLong);
    }
    // Letters from any script (Arabic, accented French...), digits, space, - _ .
    if !nickname
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
    {
        return Err(NicknameError::BadCharacters);
    }
    // A run of digits this long is probably a phone number or birth date.
    if nickname.chars().filter(char::is_ascii_digit).count() > 4 {
        return Err(NicknameError::PersonalInfo);
    }
    let normalized = normalize(&nickname);
    if BLOCKLIST.iter().any(|bad| {
        // Very short entries only match as whole words, to avoid false positives.
        if bad.len() <= 2 {
            normalized.split(' ').any(|word| word == *bad)
        } else {
            normalized.replace(' ', "").contains(bad)
        }
    }) {
        return Err(NicknameError::Blocked);
    }
    Ok(nickname)
}

/// Lowercases, undoes common leetspeak and strips accents/separators so "Sh1t", "S.H.I.T"
/// and "shît" all normalize to "shit". Spaces are kept as word boundaries.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let mapped = match c {
            '0' => 'o',
            '1' | '!' => 'i',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' => 't',
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            ' ' => ' ',
            c if c.is_alphanumeric() => c,
            _ => continue, // drop '.', '-', '_'
        };
        out.push(mapped);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        for name in [
            "Yasmine",
            "Hassan",
            "Hercule",
            "Élodie",
            "ياسمين",
            "Lion_King 7",
            "Dominique",
            "Nazih",
            "Computer",
            "Grape",
            "Peacock",
            "Fukuda",
            "Skill",
            "Updated",
        ] {
            assert_eq!(
                validate(name).as_deref(),
                Ok(name),
                "{name} should be allowed"
            );
        }
    }

    #[test]
    fn trims_and_collapses_whitespace() {
        assert_eq!(validate("  Sami   Ben  ").as_deref(), Ok("Sami Ben"));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(validate("   "), Err(NicknameError::Empty));
        assert_eq!(validate("abcdefghijklmnopq"), Err(NicknameError::TooLong));
        assert_eq!(validate("me@mail.com"), Err(NicknameError::BadCharacters));
        assert_eq!(validate("<script>"), Err(NicknameError::BadCharacters));
        assert_eq!(validate("Sami 98765432"), Err(NicknameError::PersonalInfo));
    }

    #[test]
    fn rejects_blocklisted_words_and_disguises() {
        for name in [
            "shit", "Sh1t", "S.H.I.T", "big fuck", "Putain", "enculé", "pd", "B1TCH",
        ] {
            assert_eq!(
                validate(name),
                Err(NicknameError::Blocked),
                "{name} should be blocked"
            );
        }
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        // 16 Arabic letters = 32 bytes but 16 characters: allowed.
        assert!(validate("سسسسسسسسسسسسسسسس").is_ok());
    }
}
