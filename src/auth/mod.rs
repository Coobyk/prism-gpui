//! Authentication. Mirrors launcher/minecraft/auth/*.
//! Microsoft device-code + Xbox Live + XSTS + Minecraft services.

#[derive(Debug, Clone, Default)]
pub struct Account {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

pub struct AuthService;

impl AuthService {
    /// Start device code flow; returns (user_code, verification_uri, device_code).
    pub fn start_device_code() -> Result<(String, String, String), String> {
        // Placeholder: real impl uses reqwest to
        // https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode
        Ok(("XXXX-XXXX".to_string(), "https://microsoft.com/link".to_string(), "dc".to_string()))
    }

    pub fn poll_token(_device_code: &str) -> Result<Account, String> {
        Err("not implemented".to_string())
    }
}
