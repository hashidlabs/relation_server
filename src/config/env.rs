use core::fmt;

use serde::Deserialize;

#[derive(Default, Clone, Debug, Deserialize)]
pub enum ENV {
    #[default]
    Development,
    Testing,
    Staging,
    Production,
}

impl fmt::Display for ENV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ENV::Development => write!(f, "development"),
            ENV::Testing => write!(f, "testing"),
            ENV::Staging => write!(f, "staging"),
            ENV::Production => write!(f, "production"),
        }
    }
}

impl From<String> for ENV {
    fn from(env: String) -> Self {
        match env.as_str() {
            "development" => ENV::Development,
            "production" => ENV::Production,
            "testing" => ENV::Testing,
            _ => ENV::Development,
        }
    }
}
