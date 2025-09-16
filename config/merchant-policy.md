# Merchant Policy API Notes

This document summarizes how the OnlyKAS merchant HTTP server protects the
policy-management endpoints that were added for runtime configuration.

## Authentication

Policy endpoints accept either of the following credentials:

- **API key** – include `X-API-Key: <token>` in the request headers. The token
  must match the value supplied on `kdapp-merchant serve --api-key`.
- **Session token** – present a token issued by an external authentication flow
  using one of these channels:
  - `Authorization: Bearer <token>`
  - `X-Session-Token: <token>`
  - `Cookie: merchant_session=<token>`

Session tokens are hashed with SHA-256 before being stored in sled. Use
`storage::store_session_token` (or an equivalent administrative task) to seed
valid session handles. Tokens can be revoked with
`storage::remove_session_token`. Requests without a recognised credential are
rejected with `401 Unauthorized`.

## Script template schema

`POST /policy/templates` upserts script templates that the episode enforces
against incoming payments. Payloads must conform to the JSON schema below:

```json
{
  "template_id": "merchant_p2pk",
  "script_hex": "4104...ac",
  "description": "Optional operator notes"
}
```

Validation rules:

- `template_id` **must** be one of `merchant_p2pk`,
  `merchant_guardian_multisig`, or `merchant_taproot`.
- `script_hex` must decode to non-empty bytes. The server normalizes pushes so
  equivalent encodings collapse to a canonical form before storage.
- `description` is optional metadata that is persisted verbatim.

Templates can be listed via `GET /policy/templates` and deleted with
`DELETE /policy/templates/{template_id}`. All policy routes require the
authentication described above and will return `403 Forbidden` when a template
identifier falls outside the whitelist.
