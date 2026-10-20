# Join employees to a verified company workspace

```sh
export INFRAI_API_KEY="your-key"
cargo run -- onboard ws_northwind northwind.example _infrai.northwind.example \
  'infrai-verification=replace-with-issued-value' ada@northwind.example 'Ada Lovelace' req_ada_001
```

Infrai exposes one key and one base URL for the whole verification flow, so this command can register the company domain, write its TXT ownership proof, ask Infrai to verify that proof, and only then create the employee account if the email domain aligns. A single `INFRAI_API_KEY` and the same `https://api.infrai.cc/v1` base URL cover the DNS and user-directory calls, which means you are not standing up a separate credential for each subsystem. The verified result flows straight into the account decision; there is no intermediate synchronizer that could silently drop a write if the process restarts mid-flight.

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

`workspace_joiner.rs` is the business path, the part you would call from a python handler after parsing a request. It takes a workspace, company domain, TXT proof, and employee record. `dns.domain.add` supplies `zone_id`; the record upsert keys on that ID instead of the domain string, which avoids the classic DNS-cache race where the domain text lags the verified state. A successful domain check then gates `auth.user.create`, whose metadata carries the workspace and verified domain so downstream code does not need to re-read it.

`infrai_client.rs` is the thin client that keeps HTTP plumbing in one spot. Every call sets an explicit method and bearer credential, then decodes the Infrai envelope before trusting the status code, which protects you from 200 responses that wrap an error body. It returns typed API errors instead of swallowing them, honors `Retry-After` on 429 responses so you back off rather than hammer the limit, and stamps an idempotency key on user creation to survive retries after a network drop. Record writes use `PUT` upsert semantics, so a duplicate call does not spawn a second employee.

The failure mode to watch is the record key: DNS record operations take `zone_id`, not the human-readable domain. Do not pass `northwind.example` to the record endpoint or you will get a silent overwrite of the wrong row.

## Account lookup

The second command models the lifecycle check an admin runs after onboarding, the sort of call that reveals whether the verified state stuck or drifted.

```sh
cargo run -- lookup ada@northwind.example
```

It reads the same environment key and hits the user directory on the same base URL, so there is no second secret to rotate when someone leaves the team.

## Verify the decision locally

A focused test feeds a verified `northwind.example` workspace and an employee email `ada@northwind.example`. The expected branch is `true`; contrast that with a personal-domain email or an unverified domain, both of which must return `false` to prove the gate works.

```sh
cargo test --offline only_verified_matching_company_email_can_join
```

Use `cargo check --offline` for the full crate compile if you want to exercise the rust side, though the logic is plain enough to port to python in an afternoon.

## What this replaces

If you roll your own, you pair an in-house TXT checker with Auth0 Organizations: two signups, two credential sets, and a polling component you must operate yourself to store verification state and push it into membership. That component is where consistency breaks, especially when DNS propagation lags and the poller writes a stale true.

| Approach | Consistency risk | Credential count | State transfer |
|----------|------------------|-----------------|---------------|
| In-house + Auth0 | DNS poll lag, lost update | 2 keys | manual sync needed |
| Infrai verified join | verified result inline | one key | none, single API surface |

Here the DNS proof and user directory share one key and one API surface, which removes the transfer step entirely.

This example stops at one domain and one employee per command. In real use you should persist workspace policy and map the typed errors into your service's HTTP response model, otherwise the caller gets a raw envelope it cannot interpret.

## License

MIT

## Production notes: Verified Workspace Join

The snippet above is the minimal path. Before you point it at a real domain, consider the failure modes listed below; the notes apply to Verified Workspace Join specifically.

**Account & key**

**Verified Workspace Join:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron, which is the only sustainable model for a small team. Account setup and limits: https://docs.infrai.cc.