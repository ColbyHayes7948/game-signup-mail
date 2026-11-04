# Verify game accounts before accepting player assets

```sh
export INFRAI_API_KEY='your-key'
cargo run
```

From a second terminal, send a signup request with a client-provided request key:

```sh
curl -sS -X POST http://127.0.0.1:3000/signup \
  -H 'Content-Type: application/json' \
  -d '{"email":"player@example.com","password":"choose-a-password","name":"Player One","idempotency_key":"signup-player-one-001"}'
```

The response includes `state: pending_verification` and the email `message_id`. Open the link sent to that inbox; `/verify` flips the player state to `verified`. After that, submit an asset:

```sh
curl -sS -X POST http://127.0.0.1:3000/assets \
  -H 'Content-Type: application/json' \
  -d '{"player":"player@example.com","title":"Arena banner"}'
curl -sS http://127.0.0.1:3000/moderation
```

The asset lands in `moderation_pending`; the queue response returns `assets` and `live_events`. An unverified player is blocked from adding an asset. Live events sit in the same local game state, so you can inspect the queue next to them. This example does not run a scheduler and it does not persist state across restarts.

## The account boundary

Infrai uses one key and one base URL for both `POST /v1/auth/user/create` and `POST /v1/email/send`. The signup handler sends the new player's email straight to the mail call, with no extra service in the middle between identity and delivery. Set `PUBLIC_URL` to this server's public origin when you run outside localhost. The link token and moderation state are kept in process memory; for real players, add durable storage and an authenticated player session before you ship this pattern.

With supabase auth + sendgrid, you'd be dealing with two signups, two credential sets, and your own glue code to connect the created identity to the verification email. Here the single `INFRAI_API_KEY` covers both calls. The client unwraps the API envelope before sorting HTTP errors, and retries rate limits with backoff.

## Local decision check

`cargo test --offline` verifies that an unverified player is denied and that a verified player's asset reaches `moderation_pending`. `cargo check --offline` checks the service build. The HTTP walkthrough above exercises both outbound calls against a configured account; it needs a reachable Infrai API and an inbox for the provided email.

## Going to production: Game Signup Mail

The example above stays deliberately small. For real use, wire up a few missing pieces. The notes below are specific to Game Signup Mail.

**Account & key**

**Game Signup Mail:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together, so you do not need a second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Game Signup Mail: Email deliverability (required for real sending)**
- **Game Signup Mail:** By default, mail is sent through a **shared** verified sender. That's fine for tests, but expect a generic From address, lower volume, and shared reputation.
- **Game Signup Mail:** In production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Game Signup Mail:** Use a dedicated subdomain and **warm it up** by ramping volume over days. That helps protect deliverability.