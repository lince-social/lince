# Self-Hosted Email Verification and 2FA

## Goal

Add email verification and two-factor authentication to Lince without relying on third-party auth or messaging services.

This means:

- Lince generates and verifies its own auth state
- the server sends its own email
- 2FA secrets and verification state stay in Lince-controlled storage

## Recommendation

Split the problem into two separate features:

1. `email verification`
2. `2FA`

They solve different security problems and should not be merged into a single mechanism.

Recommended stack:

- email verification: self-hosted outbound SMTP
- primary 2FA: TOTP
- stronger future 2FA: WebAuthn

Do **not** use email codes as the main second factor.

## Why

### Email verification

Email verification proves mailbox ownership.

It answers:

- can this user receive mail at this address?
- does the user control that mailbox?

It does **not** provide a strong second factor.

### 2FA

2FA proves possession of something beyond the password.

For Lince, the practical options are:

- TOTP app
- WebAuthn security key / passkey

Email-only login codes are weaker because they collapse identity recovery and second factor into the same mailbox.

## Recommended Implementation Order

1. email verification via self-hosted SMTP
2. TOTP 2FA
3. backup recovery codes
4. WebAuthn
5. optional policy enforcement for admin users

## Email Verification Design

### Flow

1. user signs up or changes email
2. server generates a random verification token
3. server stores only the token hash and expiration
4. server sends a verification link by email
5. user opens the link
6. server hashes the provided token and matches it
7. server marks the email as verified

Example URL:

```text
https://global.lince.social/verify-email?token=...
```

### Storage

Suggested fields:

- `email TEXT`
- `email_verified_at TIMESTAMP NULL`
- `email_verification_token_hash TEXT NULL`
- `email_verification_expires_at TIMESTAMP NULL`

### Requirements

- generate at least 32 random bytes for the raw token
- store only the hash, never the raw token
- make tokens one-time use
- expire tokens after 15 to 60 minutes
- support resend with rate limiting
- invalidate older pending verification tokens when a new one is issued

### Operational Requirement: Mail Delivery

If Lince sends verification mail itself, the server needs a real outbound mail setup.

Minimum serious setup:

- Postfix for outbound SMTP
- OpenDKIM for DKIM signing
- proper DNS:
  - `A`
  - `MX` if receiving mail too
  - `SPF`
  - `DKIM`
  - `DMARC`
  - reverse DNS / PTR

Without this, verification mail will frequently land in spam or be rejected.

### Suggested Mail Host Layout

- app host: `global.lince.social`
- mail host: `mail.lince.social`

The app should send transactional mail from something like:

- `no-reply@lince.social`
- `auth@lince.social`

## TOTP 2FA Design

### Flow

1. authenticated user enables 2FA
2. server generates a TOTP secret
3. server shows QR code and manual secret
4. user scans it in an authenticator app
5. user submits one current TOTP code
6. server verifies it
7. server marks 2FA as enabled
8. future logins require password + TOTP

### Storage

Suggested fields:

- `totp_enabled INTEGER NOT NULL DEFAULT 0`
- `totp_secret_encrypted TEXT NULL`
- `totp_confirmed_at TIMESTAMP NULL`

For backup codes, either:

- a separate table, or
- structured JSON if the surrounding schema already uses that pattern

Suggested backup-code storage:

- `user_id`
- `code_hash`
- `used_at`
- `created_at`

### Requirements

- encrypt the TOTP secret at rest
- allow a small clock drift window, usually plus/minus one time step
- generate backup recovery codes
- store only backup code hashes
- require re-authentication before disabling 2FA
- rate limit failed TOTP attempts

### Login State Model

The login flow should explicitly model partial authentication:

1. password accepted
2. second factor pending
3. second factor satisfied
4. full session issued

Do not issue the final session before second-factor completion.

## WebAuthn Design

### Why add it

WebAuthn is stronger than TOTP:

- phishing resistant
- no shared secret to steal from the server
- better long-term admin security

### Flow

1. authenticated user starts WebAuthn registration
2. server creates a registration challenge
3. browser talks to authenticator
4. server stores the credential public key and metadata
5. future login or step-up auth uses a challenge-response flow

### Storage

Suggested fields or table columns:

- `credential_id`
- `public_key`
- `sign_count`
- `transports`
- `created_at`
- `last_used_at`

## What Not To Do

Do not implement:

- SMS 2FA from the same server
- email as the main second factor
- a homegrown OTP algorithm
- plaintext TOTP secret storage
- unhashed backup codes
- unlimited verification or login attempts

## Security Requirements

### Rate Limiting

Apply rate limits to:

- signup
- resend verification email
- verify-email endpoint
- login
- TOTP verification
- backup-code attempts

### Auditability

Record security-relevant events:

- verification email sent
- email verified
- TOTP enabled
- TOTP disabled
- backup code used
- WebAuthn credential registered
- repeated login failures

### Recovery

Minimum recovery design:

- backup codes for TOTP/WebAuthn loss
- admin-controlled recovery path for self-hosted deployments
- explicit high-friction recovery process for privileged accounts

## Suggested Implementation Boundaries in Lince

### Persistence

Add:

- user email verification state
- token hash + expiry
- TOTP secret storage
- backup code storage
- optional WebAuthn credential table

### Application

Add:

- token generation
- mail dispatch service
- TOTP setup and verification
- backup code lifecycle
- partial-auth session handling
- security event logging

### Web

Add endpoints/pages for:

- sign up with email
- resend verification
- verify email token
- enable TOTP
- confirm TOTP
- disable TOTP
- use backup code
- register WebAuthn credential
- verify WebAuthn assertion

## Infrastructure Requirement Summary

To keep everything self-hosted:

- run outbound SMTP on the server
- configure DNS correctly
- store auth state in the app database
- protect encryption keys for TOTP secrets
- run HTTPS everywhere

## Recommended First Cut

If the goal is a practical first implementation with controlled scope:

1. add email verification with token links
2. add TOTP setup + login verification
3. add backup codes
4. defer WebAuthn to a second phase

That gets most of the security value without turning the first implementation into an infrastructure rewrite.
