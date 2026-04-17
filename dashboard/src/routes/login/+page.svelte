<script lang="ts">
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	let email = $state('');
	let password = $state('');
	let totpCode = $state('');
	let submitting = $state(false);
	let error = $state('');
	let pendingToken = $state('');
	let needs2fa = $state(false);
	let showForgot = $state(false);
	let forgotEmail = $state('');
	let forgotSent = $state(false);
	let showPassword = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!email || !password) return;
		submitting = true;
		error = '';
		try {
			const resp = await api.post<{
				token?: string;
				requires_2fa?: boolean;
				pending_token?: string;
			}>('/v1/auth/sessions', { email, password });
			if (resp.requires_2fa && resp.pending_token) {
				pendingToken = resp.pending_token;
				needs2fa = true;
			} else if (resp.token) {
				api.setToken(resp.token);
				window.location.href = '/';
			} else {
				error = 'Unexpected response from server';
			}
		} catch (err) {
			error = err instanceof Error ? err.message : 'Invalid email or password';
		} finally {
			submitting = false;
		}
	}

	async function verify2fa(e: SubmitEvent) {
		e.preventDefault();
		if (!totpCode || !pendingToken) return;
		submitting = true;
		error = '';
		try {
			const resp = await api.post<{ token: string }>('/v1/auth/sessions/verify-2fa', {
				pending_token: pendingToken,
				code: totpCode,
			});
			api.setToken(resp.token);
			window.location.href = '/';
		} catch (err) {
			error = err instanceof Error ? err.message : '2FA verification failed';
			totpCode = '';
		} finally {
			submitting = false;
		}
	}

	async function sendReset() {
		if (!forgotEmail) return;
		try {
			await api.post('/v1/auth/password-reset/request', { email: forgotEmail });
		} catch { /* ignore */ }
		forgotSent = true;
	}

	const CHAINS = ['bitcoin','ethereum','solana','tron','arbitrum','optimism','polygon','base','avalanche','bsc'];
</script>

<svelte:head>
	<title>Sign In — MG Custody</title>
</svelte:head>

<div class="page">
	<!-- ═══ Left — Brand ═══ -->
	<aside class="brand-panel">
		<div class="brand-grid" aria-hidden="true"></div>
		<div class="brand-glow" aria-hidden="true"></div>

		<div class="brand-content">
			<div class="brand-top">
				<span class="brand-mark">
					<svg width="36" height="36" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<defs>
							<linearGradient id="auth-g" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
								<stop stop-color="#6FA8FF"/>
								<stop offset="1" stop-color="#3D6BE0"/>
							</linearGradient>
						</defs>
						<path d="M16 3L4 8v9c0 7 4.8 13.2 12 15 7.2-1.8 12-8 12-15V8l-12-5z" fill="url(#auth-g)"/>
						<path d="M12 17l2.8 2.8L21 13.5" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</span>
				<div>
					<h1>MG Custody</h1>
					<span class="brand-sub">Self-hosted infrastructure</span>
				</div>
			</div>

			<div class="brand-hero">
				<h2 class="hero-title">Institutional custody,<br/>self-operated.</h2>
				<p class="hero-desc">
					HSM-backed keys, MPC thresholds, FIDO2 approvals — deployed to your
					infrastructure. No vendor, no third-party access, no black box.
				</p>
			</div>

			<ul class="feature-list">
				<li>
					<span class="feat-icon">{@html icon('shield-check', 16)}</span>
					<div>
						<strong>Cryptographic isolation</strong>
						<p>Signer runs in a separate process with no network interface.</p>
					</div>
				</li>
				<li>
					<span class="feat-icon">{@html icon('layers', 16)}</span>
					<div>
						<strong>Multi-chain from day one</strong>
						<p>Bitcoin, Ethereum, Solana, Tron + 60 EVM L2s. Add chains via config.</p>
					</div>
				</li>
				<li>
					<span class="feat-icon">{@html icon('file-text', 16)}</span>
					<div>
						<strong>Auditable by design</strong>
						<p>Hash-chained audit log, Proof of Reserves, SOC 2-ready controls.</p>
					</div>
				</li>
			</ul>

			<div class="chain-strip">
				<span class="chain-strip-label">Supported</span>
				<div class="chain-strip-icons">
					{#each CHAINS as c}
						{@html chainIcon(c, 18)}
					{/each}
				</div>
			</div>
		</div>
	</aside>

	<!-- ═══ Right — Form ═══ -->
	<section class="form-panel">
		<div class="form-container">

			<!-- Mobile mini-brand -->
			<div class="mobile-brand">
				<span class="brand-mark sm">
					<svg width="28" height="28" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<defs>
							<linearGradient id="auth-g2" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
								<stop stop-color="#6FA8FF"/>
								<stop offset="1" stop-color="#3D6BE0"/>
							</linearGradient>
						</defs>
						<path d="M16 3L4 8v9c0 7 4.8 13.2 12 15 7.2-1.8 12-8 12-15V8l-12-5z" fill="url(#auth-g2)"/>
						<path d="M12 17l2.8 2.8L21 13.5" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</span>
				<span>MG Custody</span>
			</div>

			{#if showForgot}
				<!-- ── Forgot password ── -->
				<div class="form-header">
					<h2>Reset password</h2>
					<p>Enter your email — we'll send a reset link.</p>
				</div>

				{#if forgotSent}
					<div class="banner banner-success">
						<span class="banner-icon">{@html icon('check', 16)}</span>
						<span>If an account exists, a reset link has been sent.</span>
					</div>
					<button class="btn btn-ghost full" onclick={() => { showForgot = false; forgotSent = false; }}>
						Back to sign in
					</button>
				{:else}
					<form onsubmit={(e) => { e.preventDefault(); sendReset(); }}>
						<div class="field">
							<label for="reset-email">Email</label>
							<div class="input-wrap">
								<span class="input-icon">{@html icon('profile', 15)}</span>
								<input
									id="reset-email"
									type="email"
									bind:value={forgotEmail}
									required
									placeholder="you@company.com"
								/>
							</div>
						</div>
						<button type="submit" class="btn btn-primary full">Send reset link</button>
						<button type="button" class="btn btn-ghost full" onclick={() => { showForgot = false; }}>
							Back to sign in
						</button>
					</form>
				{/if}

			{:else if !needs2fa}
				<!-- ── Sign in ── -->
				<div class="form-header">
					<h2>Sign in</h2>
					<p>Access your custody console.</p>
				</div>

				<form onsubmit={submit}>
					{#if error}
						<div class="banner banner-error">
							<span class="banner-icon">{@html icon('alert-triangle', 16)}</span>
							<span>{error}</span>
						</div>
					{/if}

					<div class="field">
						<label for="email">Email</label>
						<div class="input-wrap">
							<span class="input-icon">{@html icon('profile', 15)}</span>
							<input
								id="email"
								type="email"
								bind:value={email}
								autocomplete="email"
								required
								placeholder="admin@custody.local"
							/>
						</div>
					</div>

					<div class="field">
						<div class="field-label-row">
							<label for="password">Password</label>
							<button type="button" class="link" onclick={() => { showForgot = true; error = ''; }}>
								Forgot password?
							</button>
						</div>
						<div class="input-wrap">
							<span class="input-icon">{@html icon('lock', 15)}</span>
							<input
								id="password"
								type={showPassword ? 'text' : 'password'}
								bind:value={password}
								autocomplete="current-password"
								required
								placeholder="Enter password"
							/>
							<button
								type="button"
								class="input-action"
								aria-label={showPassword ? 'Hide password' : 'Show password'}
								onclick={() => { showPassword = !showPassword; }}
							>
								{@html icon(showPassword ? 'eye-off' : 'eye', 15)}
							</button>
						</div>
					</div>

					<label class="checkbox-row">
						<input type="checkbox" />
						<span>Keep me signed in on this device</span>
					</label>

					<button type="submit" class="btn btn-primary full" disabled={submitting}>
						{#if submitting}
							<span class="spinner"></span>
							<span>Signing in…</span>
						{:else}
							<span>Sign in</span>
							<span class="btn-chev">{@html icon('chevron-right', 14)}</span>
						{/if}
					</button>
				</form>

				<div class="divider"><span>New here?</span></div>
				<a href="/register" class="btn btn-secondary full">
					<span>Create an account</span>
					<span>{@html icon('chevron-right', 14)}</span>
				</a>

			{:else}
				<!-- ── 2FA ── -->
				<div class="form-header">
					<span class="header-icon">{@html icon('shield-check', 20)}</span>
					<h2>Two-factor authentication</h2>
					<p>Enter the 6-digit code from your authenticator app.</p>
				</div>

				<form onsubmit={verify2fa}>
					{#if error}
						<div class="banner banner-error">
							<span class="banner-icon">{@html icon('alert-triangle', 16)}</span>
							<span>{error}</span>
						</div>
					{/if}

					<div class="field">
						<label for="totp">Authentication code</label>
						<input
							id="totp"
							type="text"
							bind:value={totpCode}
							autocomplete="one-time-code"
							inputmode="numeric"
							pattern="[0-9]*"
							maxlength="6"
							placeholder="000000"
							required
							class="totp-input mono"
						/>
					</div>

					<button type="submit" class="btn btn-primary full" disabled={submitting}>
						{#if submitting}
							<span class="spinner"></span>
							<span>Verifying…</span>
						{:else}
							<span>Verify &amp; continue</span>
						{/if}
					</button>

					<button type="button" class="btn btn-ghost full" onclick={() => { needs2fa = false; error = ''; totpCode = ''; }}>
						Use a different account
					</button>
				</form>
			{/if}

			<footer class="form-footer">
				<span>MG Custody v1.0</span>
				<span class="dot-sep">·</span>
				<span>Self-hosted</span>
				<span class="dot-sep">·</span>
				<a href="https://github.com/anthropics/claude-code" target="_blank" rel="noopener">Open source</a>
			</footer>
		</div>
	</section>
</div>

<style>
	.page {
		min-height: 100vh;
		display: flex;
		background: var(--bg);
	}

	/* ═══════════════════════ Brand panel (left) ═══════════════════════ */
	.brand-panel {
		flex: 1;
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--sp-10);
		overflow: hidden;
		background:
			radial-gradient(circle at 20% 30%, rgba(111, 168, 255, 0.07) 0%, transparent 45%),
			radial-gradient(circle at 80% 70%, rgba(61, 107, 224, 0.06) 0%, transparent 45%),
			linear-gradient(180deg, #0A0B0D 0%, #0C0D10 100%);
		border-right: 1px solid var(--border);
	}

	/* Subtle grid pattern — institutional tech feel */
	.brand-grid {
		position: absolute;
		inset: 0;
		background-image:
			linear-gradient(rgba(255,255,255,0.025) 1px, transparent 1px),
			linear-gradient(90deg, rgba(255,255,255,0.025) 1px, transparent 1px);
		background-size: 56px 56px;
		mask-image: radial-gradient(ellipse at 50% 50%, black 35%, transparent 75%);
		-webkit-mask-image: radial-gradient(ellipse at 50% 50%, black 35%, transparent 75%);
		pointer-events: none;
	}

	.brand-glow {
		position: absolute;
		top: -200px;
		left: -120px;
		width: 560px;
		height: 560px;
		background: radial-gradient(circle, rgba(111, 168, 255, 0.12) 0%, transparent 60%);
		pointer-events: none;
		filter: blur(10px);
	}

	.brand-content {
		position: relative;
		max-width: 480px;
		width: 100%;
	}

	.brand-top {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		margin-bottom: var(--sp-10);
	}

	.brand-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 48px;
		height: 48px;
		border-radius: var(--r-md);
		background: rgba(111, 168, 255, 0.08);
		border: 1px solid rgba(111, 168, 255, 0.18);
		flex-shrink: 0;
	}

	.brand-mark.sm { width: 36px; height: 36px; }

	.brand-top h1 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.015em;
	}

	.brand-sub {
		color: var(--fg-muted);
		font-size: 0.78rem;
	}

	.brand-hero { margin-bottom: var(--sp-8); }

	.hero-title {
		font-size: 2.1rem;
		font-weight: 650;
		letter-spacing: -0.03em;
		line-height: 1.15;
		color: var(--fg-strong);
		margin: 0 0 var(--sp-4);
	}

	.hero-desc {
		color: var(--fg-muted);
		font-size: 0.95rem;
		line-height: 1.55;
		margin: 0;
	}

	.feature-list {
		list-style: none;
		padding: 0;
		margin: 0 0 var(--sp-8);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.feature-list li {
		display: flex;
		gap: var(--sp-3);
		align-items: flex-start;
	}

	.feat-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: var(--r-sm);
		background: var(--accent-soft);
		color: var(--accent);
		flex-shrink: 0;
	}

	.feature-list strong {
		display: block;
		font-size: 0.9rem;
		color: var(--fg-strong);
		font-weight: 550;
		margin-bottom: 2px;
	}

	.feature-list p {
		margin: 0;
		color: var(--fg-muted);
		font-size: 0.82rem;
		line-height: 1.45;
	}

	.chain-strip {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding-top: var(--sp-5);
		border-top: 1px solid var(--border);
	}

	.chain-strip-label {
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-dim);
	}

	.chain-strip-icons {
		display: flex;
		gap: 5px;
		opacity: 0.75;
	}

	.chain-strip-icons :global(svg) { border-radius: 50%; }

	/* ═══════════════════════ Form panel (right) ═══════════════════════ */
	.form-panel {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--sp-6);
		min-width: 440px;
	}

	.form-container {
		width: 100%;
		max-width: 380px;
		display: flex;
		flex-direction: column;
	}

	.mobile-brand {
		display: none;
		align-items: center;
		gap: var(--sp-2);
		margin-bottom: var(--sp-6);
		font-weight: 600;
		color: var(--fg-strong);
		font-size: 1rem;
	}

	.form-header { margin-bottom: var(--sp-6); }
	.header-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		border-radius: var(--r-md);
		background: var(--accent-soft);
		color: var(--accent);
		margin-bottom: var(--sp-3);
	}

	.form-header h2 {
		margin: 0 0 4px;
		font-size: 1.35rem;
		font-weight: 650;
		letter-spacing: -0.02em;
		color: var(--fg-strong);
	}

	.form-header p {
		margin: 0;
		color: var(--fg-muted);
		font-size: 0.88rem;
	}

	/* Form */
	form {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field-label-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.field label {
		font-size: 0.78rem;
		font-weight: 500;
		color: var(--fg-muted);
	}

	.input-wrap {
		position: relative;
		display: flex;
		align-items: center;
	}

	.input-icon {
		position: absolute;
		left: 12px;
		color: var(--fg-dim);
		display: inline-flex;
		pointer-events: none;
	}

	.input-action {
		position: absolute;
		right: 8px;
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: var(--r-sm);
		transition: color 0.15s, background 0.15s;
	}
	.input-action:hover { color: var(--fg); background: var(--hover-bg); }

	.input-wrap input {
		flex: 1;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.9rem;
		padding: 10px 12px 10px 38px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s, background 0.15s;
	}

	.input-wrap input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}

	.input-wrap input::placeholder {
		color: var(--fg-dim);
	}

	.totp-input {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg-strong);
		font-size: 1.5rem;
		text-align: center;
		letter-spacing: 0.5rem;
		padding: 14px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.totp-input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
	}

	.checkbox-row {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.82rem;
		color: var(--fg-muted);
		cursor: pointer;
		user-select: none;
	}

	.checkbox-row input {
		width: 15px;
		height: 15px;
		accent-color: var(--accent);
		cursor: pointer;
	}

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		padding: 10px 14px;
		font-size: 0.88rem;
		font-weight: 550;
		border-radius: var(--r-md);
		border: 1px solid transparent;
		cursor: pointer;
		text-decoration: none;
		transition: background 0.15s, border-color 0.15s, transform 0.05s;
	}
	.btn:active:not(:disabled) { transform: translateY(1px); }
	.btn:disabled { opacity: 0.55; cursor: not-allowed; }
	.btn.full { width: 100%; }

	.btn-primary {
		background: var(--accent);
		color: #0A0B0D;
		font-weight: 600;
	}
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); }

	.btn-ghost {
		background: transparent;
		color: var(--fg-muted);
		border-color: transparent;
	}
	.btn-ghost:hover { color: var(--fg); background: var(--hover-bg); }

	.btn-secondary {
		background: transparent;
		color: var(--fg);
		border-color: var(--border);
	}
	.btn-secondary:hover { background: var(--hover-bg); border-color: var(--border-strong); }

	.btn-chev { display: inline-flex; }

	.link {
		background: none;
		border: none;
		color: var(--accent);
		font-size: 0.78rem;
		font-weight: 500;
		cursor: pointer;
		padding: 0;
	}
	.link:hover { color: var(--accent-strong); text-decoration: underline; }

	.spinner {
		width: 14px;
		height: 14px;
		border: 2px solid #0A0B0D;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* Banner */
	.banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 12px;
		border-radius: var(--r-sm);
		font-size: 0.82rem;
	}
	.banner-error {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.banner-success {
		background: var(--green-soft);
		color: var(--green);
		border: 1px solid rgba(52, 211, 153, 0.25);
		margin-bottom: var(--sp-3);
	}
	.banner-icon { display: inline-flex; flex-shrink: 0; }

	/* Divider */
	.divider {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		margin: var(--sp-5) 0 var(--sp-3);
		font-size: 0.72rem;
		color: var(--fg-dim);
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}

	.divider::before, .divider::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--border);
	}

	.hint {
		text-align: center;
		color: var(--fg-muted);
		font-size: 0.82rem;
		margin: 0;
	}

	/* Footer */
	.form-footer {
		margin-top: var(--sp-8);
		text-align: center;
		font-size: 0.72rem;
		color: var(--fg-dim);
		display: flex;
		justify-content: center;
		gap: 8px;
	}

	.dot-sep { opacity: 0.5; }

	.form-footer a { color: var(--fg-dim); }
	.form-footer a:hover { color: var(--fg-muted); }

	/* ═══════════════════════ Mobile ═══════════════════════ */
	@media (max-width: 960px) {
		.page { flex-direction: column; }
		.brand-panel { display: none; }
		.form-panel { min-width: auto; padding: var(--sp-5); }
		.mobile-brand { display: flex; }
	}
</style>
