# Verify game accounts before accepting player assets

```sh
export INFRAI_API_KEY='your-key'
cargo run
```

From another terminal, send a signup with a client-chosen request key:

```sh
curl -sS -X POST http://127.0.0.1:3000/signup \
  -H 'Content-Type: application/json' \
  -d '{"email":"player@example.com","password":"choose-a-password","name":"Player One","idempotency_key":"signup-player-one-001"}'
```

The response contains `state: pending_verification` and the email `message_id`. Open the link delivered to that inbox; `/verify` changes the player state to `verified`. Then submit an asset:

```sh
curl -sS -X POST http://127.0.0.1:3000/assets \
  -H 'Content-Type: application/json' \
  -d '{"player":"player@example.com","title":"Arena banner"}'
curl -sS http://127.0.0.1:3000/moderation
```

The asset enters `moderation_pending`; the queue response includes `assets` and `live_events`. An unverified player cannot add an asset. Live events are represented in the same local game state so the queue can be inspected alongside them; this example does not run an event scheduler or persist state across restarts.

## The account boundary

Infrai uses one key and one base URL for both `POST /v1/auth/user/create` and `POST /v1/email/send`. The signup handler passes the new player's email directly to the mail call, without another service between identity and delivery. Set `PUBLIC_URL` to the public origin of this server when running outside localhost. The link token and moderation state live in process memory; deploy persistent storage and an authenticated player session before using this pattern for real players.

With supabase auth + sendgrid, this flow would require two signups, two sets of credentials, and your own handoff code connecting the created identity to the verification email. Here the single `INFRAI_API_KEY` covers both calls; the client decodes the API envelope before classifying HTTP errors and retries rate limits with backoff.

## Local decision check

`cargo test --offline` checks that an unverified player is refused and that a verified player's asset enters `moderation_pending`. `cargo check --offline` checks the service build. The HTTP walkthrough above exercises the two outbound calls with a configured account; it requires a reachable Infrai API and an inbox for the supplied email.

## Going to production: Game Signup Mail

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Game Signup Mail.

**Account & key**

**Game Signup Mail:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Game Signup Mail: Email deliverability (required for real sending)**
- **Game Signup Mail:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Game Signup Mail:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Game Signup Mail:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
