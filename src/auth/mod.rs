//! Authentication. Mirrors launcher/minecraft/auth/*.
//! Microsoft device-code → Xbox Live → XSTS → Minecraft services.

use serde::{Deserialize, Serialize};

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

pub const DEFAULT_CLIENT_ID: &str = "00000000402b5328";

#[derive(Debug, Clone, Default)]
pub struct Account {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct MsToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct XblProps {
    #[serde(rename = "AuthMethod")]
    auth_method: String,
    #[serde(rename = "SiteName")]
    site_name: String,
    #[serde(rename = "RpsTicket")]
    rps_ticket: String,
}

#[derive(Debug, Serialize)]
struct XblRequest {
    #[serde(rename = "Properties")]
    properties: XblProps,
    #[serde(rename = "RelyingParty")]
    relying_party: String,
    #[serde(rename = "TokenType")]
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Debug, Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Debug, Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct McLoginResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct McProfile {
    id: String,
    name: String,
}

pub struct AuthService {
    client_id: String,
}

impl Default for AuthService {
    fn default() -> Self {
        Self {
            client_id: std::env::var("PRISM_MS_CLIENT_ID")
                .unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string()),
        }
    }
}

impl AuthService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start device code flow.
    pub fn start_device_code(&self) -> Result<DeviceCode, String> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ];
        let resp = ureq::post(DEVICE_CODE_URL)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_form(&params)
            .map_err(|e| e.to_string())?;
        resp.into_json::<DeviceCode>().map_err(|e| e.to_string())
    }

    /// Poll for a token after the user completes sign-in.
    pub fn poll_token(&self, device_code: &str) -> Result<MsToken, String> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ];
        let resp = ureq::post(TOKEN_URL)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_form(&params)
            .map_err(|e| e.to_string())?;
        resp.into_json::<MsToken>().map_err(|e| e.to_string())
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<MsToken, String> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", "XboxLive.signin offline_access"),
        ];
        let resp = ureq::post(TOKEN_URL)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_form(&params)
            .map_err(|e| e.to_string())?;
        resp.into_json::<MsToken>().map_err(|e| e.to_string())
    }

    /// Full chain: Microsoft token → Xbox → XSTS → Minecraft profile.
    pub fn login_with_ms_token(&self, ms_token: &str) -> Result<Account, String> {
        let xbl = XblRequest {
            properties: XblProps {
                auth_method: "RPS".into(),
                site_name: "user.auth.xboxlive.com".into(),
                rps_ticket: format!("d={ms_token}"),
            },
            relying_party: "http://auth.xboxlive.com".into(),
            token_type: "JWT".into(),
        };
        let xbl_resp: XblResponse = ureq::post(XBL_URL)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_json(xbl)
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        let uhs = xbl_resp
            .display_claims
            .xui
            .first()
            .map(|x| x.uhs.clone())
            .ok_or("missing XUI claim")?;

        let xsts = serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_resp.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        });
        let xsts_resp: XblResponse = ureq::post(XSTS_URL)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_json(xsts)
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;

        let payload = serde_json::json!({
            "identityToken": format!("XBL3.0 x={};{}", uhs, xsts_resp.token)
        });
        let mc: McLoginResponse = ureq::post(MC_LOGIN_URL)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_json(payload)
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;

        let profile: McProfile = ureq::get(MC_PROFILE_URL)
            .set("Authorization", &format!("Bearer {}", mc.access_token))
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;

        Ok(Account {
            username: profile.name,
            uuid: profile.id,
            access_token: mc.access_token,
            refresh_token: String::new(),
            expires_at: 0,
        })
    }

    /// Run the whole device-code flow: print the user code, then poll until the
    /// user authorizes, and return the resulting account.
    pub fn login_interactive<F>(&self, mut on_prompt: F) -> Result<Account, String>
    where
        F: FnMut(&DeviceCode),
    {
        let dc = self.start_device_code()?;
        on_prompt(&dc);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(dc.expires_in);
        let interval = std::time::Duration::from_secs(dc.interval.max(1));
        while std::time::Instant::now() < deadline {
            std::thread::sleep(interval);
            match self.poll_token(&dc.device_code) {
                Ok(token) => return self.login_with_ms_token(&token.access_token),
                Err(_) => continue,
            }
        }
        Err("device code expired before authorization".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_client_id_present() {
        assert!(!DEFAULT_CLIENT_ID.is_empty());
    }
}
