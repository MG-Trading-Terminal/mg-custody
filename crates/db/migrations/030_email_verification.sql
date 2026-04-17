-- Track which workspace_users have confirmed their email address.
--
-- NULL ⇒ unverified. Set to NOW() when the user successfully consumes an
-- OTP with purpose = 'email_verify'. Once set, we do not clear it — if
-- the user later changes their email, the application layer is
-- responsible for resetting this column as part of that flow.
--
-- No backfill: existing accounts remain NULL and will be nudged to
-- verify on their next login (by the UI reading the `email_verified`
-- flag on the session response).

ALTER TABLE workspace_users
    ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ;
