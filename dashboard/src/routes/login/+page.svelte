<script lang="ts">
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';

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
				error = 'Unexpected response';
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
</script>

<svelte:head>
	<title>Sign In — MG Custody</title>
</svelte:head>

<div class="page">
	<!-- Left panel — branding -->
	<div class="brand-panel">
		<div class="brand-content">
			<div class="logo-area">
				<svg width="48" height="48" viewBox="0 0 48 48" fill="none">
					<path d="M24 4L6 12v12c0 11 7.7 21.3 18 24 10.3-2.7 18-13 18-24V12L24 4z" fill="url(#lg)" opacity="0.9"/>
					<path d="M20 27l-5-5 2.5-2.5L20 22l7.5-7.5L30 17 20 27z" fill="#fff"/>
					<defs><linearGradient id="lg" x1="6" y1="4" x2="42" y2="48"><stop stop-color="#58a6ff"/><stop offset="1" stop-color="#1f6feb"/></linearGradient></defs>
				</svg>
				<h1>MG Custody</h1>
			</div>
			<p class="tagline">Self-hosted cryptocurrency custody for institutions</p>

			<div class="features">
				<div class="feature">
					<span class="feat-icon">
						<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
					</span>
					<div>
						<strong>Enterprise Security</strong>
						<p>HSM key storage, MPC signing, FIDO2 hardware keys</p>
					</div>
				</div>
				<div class="feature">
					<span class="feat-icon">
						<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/></svg>
					</span>
					<div>
						<strong>74 Chains, 800+ Tokens</strong>
						<p>Bitcoin, Ethereum, Solana, Tron, TON + 60 EVM L2s</p>
					</div>
				</div>
				<div class="feature">
					<span class="feat-icon">
						<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
					</span>
					<div>
						<strong>Full Control</strong>
						<p>Self-hosted — your keys, your infrastructure, zero vendor lock-in</p>
					</div>
				</div>
			</div>

			<div class="chain-row">
				{@html chainIcon('bitcoin', 20)}
				{@html chainIcon('ethereum', 20)}
				{@html chainIcon('solana', 20)}
				{@html chainIcon('tron', 20)}
				{@html chainIcon('arbitrum', 20)}
				{@html chainIcon('optimism', 20)}
				{@html chainIcon('polygon', 20)}
				{@html chainIcon('base', 20)}
				{@html chainIcon('avalanche', 20)}
				{@html chainIcon('bsc', 20)}
			</div>
		</div>
	</div>

	<!-- Right panel — form -->
	<div class="form-panel">
		<div class="form-container">
			{#if showForgot}
				<!-- Forgot password -->
				<h2>Reset Password</h2>
				<p class="form-sub">Enter your email to receive a reset link</p>

				{#if forgotSent}
					<div class="success-msg">If an account exists, a reset link has been sent.</div>
					<button class="link-btn" onclick={() => { showForgot = false; forgotSent = false; }}>Back to login</button>
				{:else}
					<form onsubmit={(e) => { e.preventDefault(); sendReset(); }}>
						<label>
							Email
							<input type="email" bind:value={forgotEmail} required placeholder="you@company.com" />
						</label>
						<button type="submit" class="btn-primary">Send Reset Link</button>
						<button type="button" class="link-btn" onclick={() => { showForgot = false; }}>Back to login</button>
					</form>
				{/if}

			{:else if !needs2fa}
				<!-- Login form -->
				<h2>Welcome back</h2>
				<p class="form-sub">Sign in to your custody account</p>

				<form onsubmit={submit}>
					{#if error}
						<div class="err">{error}</div>
					{/if}

					<label>
						Email
						<input type="email" bind:value={email} autocomplete="email" required placeholder="admin@custody.local" />
					</label>

					<label>
						Password
						<input type="password" bind:value={password} autocomplete="current-password" required placeholder="Enter password" />
					</label>

					<div class="form-row-between">
						<label class="remember">
							<input type="checkbox" /> Remember me
						</label>
						<button type="button" class="link-btn" onclick={() => { showForgot = true; error = ''; }}>
							Forgot password?
						</button>
					</div>

					<button type="submit" class="btn-primary" disabled={submitting}>
						{#if submitting}
							<span class="spinner"></span> Signing in...
						{:else}
							Sign in
						{/if}
					</button>
				</form>

				<div class="divider"><span>or</span></div>

				<p class="register-hint">
					Need an account? Contact your administrator.
				</p>

			{:else}
				<!-- 2FA -->
				<h2>Two-Factor Authentication</h2>
				<p class="form-sub">Enter the 6-digit code from your authenticator app</p>

				<form onsubmit={verify2fa}>
					{#if error}
						<div class="err">{error}</div>
					{/if}

					<label>
						Authentication Code
						<input
							type="text"
							bind:value={totpCode}
							autocomplete="one-time-code"
							inputmode="numeric"
							pattern="[0-9]{6}"
							maxlength="6"
							placeholder="000 000"
							required
							class="totp-input"
						/>
					</label>

					<button type="submit" class="btn-primary" disabled={submitting}>
						{submitting ? 'Verifying...' : 'Verify'}
					</button>

					<button type="button" class="link-btn" onclick={() => { needs2fa = false; error = ''; }}>
						Use a different account
					</button>
				</form>
			{/if}

			<div class="footer">
				<span>MG Custody v1.0</span>
				<span class="sep">|</span>
				<span>Self-Hosted</span>
				<span class="sep">|</span>
				<span>Open Source</span>
			</div>
		</div>
	</div>
</div>

<style>
	.page {
		min-height: 100vh;
		display: flex;
		background: var(--bg, #0f1117);
	}

	/* ── Left branding panel ── */
	.brand-panel {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		background: linear-gradient(135deg, #0d1117 0%, #161b22 50%, #0d1117 100%);
		border-right: 1px solid var(--border, #30363d);
		padding: 3rem;
	}

	.brand-content {
		max-width: 420px;
	}

	.logo-area {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}

	.logo-area h1 {
		margin: 0;
		font-size: 1.6rem;
		color: var(--accent, #58a6ff);
	}

	.tagline {
		color: var(--fg-muted, #8b949e);
		font-size: 0.95rem;
		margin: 0 0 2.5rem;
	}

	.features {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		margin-bottom: 2rem;
	}

	.feature {
		display: flex;
		gap: 0.75rem;
		align-items: flex-start;
	}

	.feat-icon {
		color: var(--accent, #58a6ff);
		flex-shrink: 0;
		margin-top: 2px;
	}

	.feature strong {
		display: block;
		font-size: 0.9rem;
		margin-bottom: 0.15rem;
		color: var(--fg, #e1e4e8);
	}

	.feature p {
		margin: 0;
		font-size: 0.8rem;
		color: var(--fg-muted, #8b949e);
		line-height: 1.4;
	}

	.chain-row {
		display: flex;
		gap: 0.5rem;
		opacity: 0.6;
	}

	.chain-row :global(svg) { border-radius: 50%; }

	/* ── Right form panel ── */
	.form-panel {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
		max-width: 480px;
	}

	.form-container {
		width: 100%;
		max-width: 360px;
	}

	h2 {
		margin: 0 0 0.25rem;
		font-size: 1.4rem;
		color: var(--fg, #e1e4e8);
	}

	.form-sub {
		color: var(--fg-muted, #8b949e);
		font-size: 0.85rem;
		margin: 0 0 1.5rem;
	}

	form { display: flex; flex-direction: column; gap: 0.9rem; }

	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.82rem;
		color: var(--fg-muted, #8b949e);
		font-weight: 500;
	}

	input[type="email"],
	input[type="password"],
	input[type="text"] {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		color: var(--fg, #e1e4e8);
		font-size: 0.9rem;
		padding: 0.6rem 0.85rem;
		outline: none;
		transition: border-color 0.15s;
	}

	input:focus { border-color: var(--accent, #58a6ff); box-shadow: 0 0 0 3px rgba(88, 166, 255, 0.1); }

	input::placeholder { color: var(--fg-muted, #8b949e); opacity: 0.5; }

	.totp-input {
		font-size: 1.5rem;
		text-align: center;
		letter-spacing: 0.5rem;
		font-family: monospace;
	}

	.form-row-between {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.remember {
		flex-direction: row;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.8rem;
	}

	.remember input { width: auto; }

	.btn-primary {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 8px;
		font-size: 0.9rem;
		font-weight: 600;
		padding: 0.65rem;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		transition: opacity 0.15s;
	}

	.btn-primary:hover { opacity: 0.9; }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.spinner {
		width: 16px;
		height: 16px;
		border: 2px solid #0f1117;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.6s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }

	.link-btn {
		background: transparent;
		color: var(--accent, #58a6ff);
		border: none;
		font-size: 0.8rem;
		cursor: pointer;
		padding: 0;
	}

	.link-btn:hover { text-decoration: underline; }

	.divider {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin: 0.75rem 0;
		color: var(--fg-muted, #8b949e);
		font-size: 0.75rem;
	}

	.divider::before, .divider::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--border, #30363d);
	}

	.register-hint {
		text-align: center;
		color: var(--fg-muted, #8b949e);
		font-size: 0.82rem;
		margin: 0;
	}

	.err {
		color: var(--red, #f85149);
		font-size: 0.82rem;
		margin: 0;
		padding: 0.5rem 0.75rem;
		background: rgba(248, 81, 73, 0.1);
		border-radius: 6px;
		border: 1px solid rgba(248, 81, 73, 0.3);
	}

	.success-msg {
		color: var(--green, #3fb950);
		font-size: 0.85rem;
		padding: 0.6rem 0.8rem;
		background: rgba(63, 185, 80, 0.1);
		border-radius: 6px;
		border: 1px solid rgba(63, 185, 80, 0.3);
		margin-bottom: 0.75rem;
	}

	.footer {
		margin-top: 2.5rem;
		text-align: center;
		font-size: 0.7rem;
		color: var(--fg-muted, #8b949e);
		opacity: 0.6;
	}

	.sep { margin: 0 0.4rem; }

	/* ── Mobile ── */
	@media (max-width: 768px) {
		.page { flex-direction: column; }
		.brand-panel { display: none; }
		.form-panel { max-width: 100%; padding: 2rem 1.5rem; }

		.form-container::before {
			content: '';
			display: block;
			text-align: center;
			margin-bottom: 1.5rem;
		}
	}
</style>
