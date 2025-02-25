use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct MatchRequest {
    client_requirements: String,
}

pub fn match_consultant(_program_id: &Pubkey, accounts: &[AccountInfo], requirements: &str) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let config_acc = next_account_info(account_info_iter)?;

    let config_data = config_acc.try_borrow_data()?;
    let config = String::from_utf8(config_data.to_vec()).map_err(|_| ProgramError::InvalidAccountData)?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        msg!("Failed to create runtime: {}", e);
        ProgramError::Custom(1)
    })?;

    let result = rt.block_on(async {
        let client = Client::new();
        let request = MatchRequest {
            client_requirements: requirements.to_string(),
        };

        client
            .post(&config)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                msg!("Network error: {}", e);
                ProgramError::Custom(2)
            })?
            .text()
            .await
            .map_err(|e| {
                msg!("Response error: {}", e);
                ProgramError::Custom(3)
            })
    })?;

    msg!("Consultant matched: {}", result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::pubkey::Pubkey;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};

    const TEST_ENDPOINT: &str = "/match";

    #[tokio::test]
    #[ignore = "Requires real API for testing"]
    async fn test_match_consultant_success() {
        let mock_server = MockServer::start().await;
        let program_id = Pubkey::new_unique();
        let key_binding = Pubkey::new_unique();
        let mut lamports_binding = 0u64;
        let mut config_data = format!("{}{}", mock_server.uri(), TEST_ENDPOINT).as_bytes().to_vec();
        let config_acc = AccountInfo::new(
            &key_binding,
            false,
            true,
            &mut lamports_binding,
            &mut config_data,
            &program_id,
            false,
            0,
        );
        let accounts = vec![config_acc];

        Mock::given(method("POST"))
            .and(path(TEST_ENDPOINT))
            .respond_with(ResponseTemplate::new(200).set_body_string("Consultant matched"))
            .mount(&mock_server)
            .await;

        let res = match_consultant(&program_id, &accounts, "Test requirements");
        assert!(res.is_ok());
    }

    #[tokio::test]
    #[ignore = "Requires real API for testing"]
    async fn test_stress_test_large_payload() {
        let mock_server = MockServer::start().await;
        let program_id = Pubkey::new_unique();
        let key_binding = Pubkey::new_unique();
        let mut lamports_binding = 0u64;
        let mut config_data = format!("{}{}", mock_server.uri(), TEST_ENDPOINT).as_bytes().to_vec();
        let config_acc = AccountInfo::new(
            &key_binding,
            false,
            true,
            &mut lamports_binding,
            &mut config_data,
            &program_id,
            false,
            0,
        );
        let accounts = vec![config_acc];

        Mock::given(method("POST"))
            .and(path(TEST_ENDPOINT))
            .respond_with(ResponseTemplate::new(200).set_body_string("Consultant matched"))
            .mount(&mock_server)
            .await;

        let large_requirements = "x".repeat(1000);
        let res = match_consultant(&program_id, &accounts, &large_requirements);
        assert!(res.is_ok());
    }
}