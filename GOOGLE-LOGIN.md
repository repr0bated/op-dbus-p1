# Google Login Integration Notes

This repository currently uses a magic-link email flow for the privacy router
signup/login endpoints exposed from `crates/op-web`. If Google login is
required, the intended integration point is the privacy API layer so that
Google OAuth can be used in place of (or in addition to) the email magic-link
flow.

## Current flow (reference)
- `POST /api/privacy/signup` creates or refreshes a magic link and emails it.
- `GET /api/privacy/verify?token=...` verifies the token and returns the VPN
  configuration payload.

## Suggested extension points
1. Add an OAuth callback handler under `crates/op-web/src/handlers`.
2. Map the Google identity (email, subject) to the existing user store.
3. Reuse the existing verification/config generation code paths once the user
   is authenticated.

This file documents where the Google login work should land when it is
implemented.
