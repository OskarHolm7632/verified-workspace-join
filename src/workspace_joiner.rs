use crate::infrai_client::{ClientError, InfraiClient};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug)]
pub struct OnboardingRequest<'a> {
    pub workspace_id: &'a str,
    pub company_domain: &'a str,
    pub proof_name: &'a str,
    pub proof_content: &'a str,
    pub employee_email: &'a str,
    pub employee_name: &'a str,
    pub idempotency_key: &'a str,
}

#[derive(Debug, Serialize)]
pub struct JoinResult {
    pub user_id: String,
    pub workspace_id: String,
    pub company_domain: String,
    pub decision: &'static str,
}

#[derive(Debug, Error)]
pub enum JoinError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("domain ownership is pending")]
    DomainPending,
    #[error("employee email does not belong to the verified company domain")]
    EmailDomainMismatch,
}

pub fn may_join(employee_email: &str, company_domain: &str, verified: bool) -> bool {
    let Some((_, email_domain)) = employee_email.rsplit_once('@') else {
        return false;
    };
    verified && email_domain.eq_ignore_ascii_case(company_domain)
}

pub async fn onboard_employee(
    client: &InfraiClient,
    request: OnboardingRequest<'_>,
) -> Result<JoinResult, JoinError> {
    let domain = client
        .add_domain(request.company_domain, request.workspace_id)
        .await?;
    client
        .upsert_txt(&domain.zone_id, request.proof_name, request.proof_content)
        .await?;
    let verification = client.verify_domain(request.company_domain).await?;

    if !verification.verified {
        return Err(JoinError::DomainPending);
    }
    if !may_join(request.employee_email, request.company_domain, true) {
        return Err(JoinError::EmailDomainMismatch);
    }

    let user = client
        .create_user(
            request.employee_email,
            request.employee_name,
            request.workspace_id,
            request.company_domain,
            request.idempotency_key,
        )
        .await?;

    Ok(JoinResult {
        user_id: user.id,
        workspace_id: request.workspace_id.to_owned(),
        company_domain: request.company_domain.to_owned(),
        decision: "joined",
    })
}

#[cfg(test)]
mod tests {
    use super::may_join;

    #[test]
    fn only_verified_matching_company_email_can_join() {
        assert!(may_join("ada@northwind.example", "northwind.example", true));
        assert!(!may_join("ada@personal.example", "northwind.example", true));
        assert!(!may_join(
            "ada@northwind.example",
            "northwind.example",
            false
        ));
    }
}
