use std::process::ExitCode;
use verified_workspace_join::infrai_client::InfraiClient;
use verified_workspace_join::workspace_joiner::{onboard_employee, OnboardingRequest};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let client = InfraiClient::from_env()?;

    match args.get(1).map(String::as_str) {
        Some("onboard") if args.len() == 9 => {
            let result = onboard_employee(
                &client,
                OnboardingRequest {
                    workspace_id: &args[2],
                    company_domain: &args[3],
                    proof_name: &args[4],
                    proof_content: &args[5],
                    employee_email: &args[6],
                    employee_name: &args[7],
                    idempotency_key: &args[8],
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Some("lookup") if args.len() == 3 => {
            let user = client.get_user_by_email(&args[2]).await?;
            println!("user {} has id {}", user.email, user.id);
        }
        _ => {
            eprintln!(
                "usage:\n  verified-workspace-join onboard WORKSPACE DOMAIN TXT_NAME TXT_CONTENT EMAIL NAME REQUEST_ID\n  verified-workspace-join lookup EMAIL"
            );
            return Ok(());
        }
    }
    Ok(())
}
