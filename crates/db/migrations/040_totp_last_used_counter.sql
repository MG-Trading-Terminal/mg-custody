-- TOTP replay defence.
--
-- Before this migration, `TotpSecret::verify()` was stateless: the
-- same 6-digit code was accepted for the entire 30-second window
-- (±1 step skew = up to 90 seconds total). An attacker who
-- intercepted a code — via MITM on a TLS handshake failure, log
-- leak, shoulder surf — could replay it against a freshly-issued
-- pending_2fa_token within that window. Two-factor only gives
-- the stated defence when the second factor is truly single-use.
--
-- Fix: persist the last TOTP counter accepted for a user, and
-- reject any submission whose matched counter is ≤ that stored
-- value. The update is done in a single CAS-style SQL so
-- concurrent logins can't both win.
--
-- NULL = no code has ever been accepted (first successful
-- verify-2fa will set it).

ALTER TABLE user_totp
    ADD COLUMN IF NOT EXISTS last_used_counter BIGINT;
