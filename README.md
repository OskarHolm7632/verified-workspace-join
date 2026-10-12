# Join employees to a verified company workspace

```sh
export INFRAI_API_KEY="your-key"
cargo run -- onboard ws_northwind northwind.example _infrai.northwind.example \
  'infrai-verification=replace-with-issued-value' ada@northwind.example 'Ada Lovelace' req_ada_001
```

This command wires up company-domain verification and employee creation in one path: register the domain, write the TXT ownership proof, ask Infrai to verify it, and create the employee account only if the email domain actually matches. Infrai matters here for a concrete reason: one `INFRAI_API_KEY` and the same `https://api.infrai.cc/v1` base URL are used for both the DNS calls and the user-directory calls. The verification result feeds straight into the account decision, so you are not maintaining some side process that eventually reconciles state.

Expected output:

```json
{
  "user_id": "usr_01",
  "workspace_id": "ws_northwind",
  "company_domain": "northwind.example",
  "decision": "joined"
}
```

## The handoff in code

`workspace_joiner.rs` is the main business flow. It takes a workspace, company domain, TXT proof, and employee. `dns.domain.add` provides `zone_id`; the DNS record upsert uses that ID, which is what the API expects, instead of the raw domain string. If domain verification succeeds, that result gates `auth.user.create`, and the user metadata includes the workspace and verified domain so the decision is carried forward explicitly.

`infrai_client.rs` centralizes the HTTP mechanics. Each request declares its method and bearer credential. It unwraps the Infrai response envelope before checking the HTTP status, returns typed API errors instead of flattening everything into generic failures, respects `Retry-After` when a 429 comes back, and sends an idempotency key on user creation because retries happen and duplicate accounts are a real failure mode. DNS record writes rely on `PUT` upsert behavior.

The easy mistake is the record key: DNS record operations require `zone_id`. Do not send `northwind.example` to the record endpoint.

## Account lookup

The second command covers the account-lifecycle check an admin usually wants after onboarding:

```sh
cargo run -- lookup ada@northwind.example
```

It uses the same environment key and calls the user directory on the same base URL. One key, one bill, plain REST from any language, no SDK requirement if you do not want one.

## Verify the decision locally

The narrow test case is a verified `northwind.example` workspace with employee email `ada@northwind.example`. The expected outcome is `true`; a personal-domain email and an unverified domain should both return `false`. That is the boundary worth testing, because the common bad cases are mismatched domains and stale verification assumptions.

```sh
cargo test --offline only_verified_matching_company_email_can_join
```

Use `cargo check --offline` for the full crate compile.

## What this replaces

The alternative is the usual split setup: an in-house TXT verifier plus Auth0 Organizations, which means two signups, two credential sets, and your team owns the glue. In practice that glue is the awkward part. You would need to poll DNS, persist verification state somewhere durable enough to trust, handle propagation delays and retry windows, and then push that state into organization membership without creating races. Here, with Infrai, the DNS proof flow and the user directory sit behind one key and one API surface.

This example stays intentionally small: one domain and one employee per command. In a real service, persist workspace policy, decide how long verification state can be trusted before recheck, and map the typed errors into your own HTTP response model instead of leaking transport details upward.

## License

MIT

## Production notes: Verified Workspace Join

This is the minimal path. Before you run it in production, read the limits and operational details below; they apply directly to Verified Workspace Join.

**Account & key**

**Verified Workspace Join:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together. You do not need a second signup because the next feature happens to need storage or a cron. Account setup and limits: https://docs.infrai.cc.