<script lang="ts">
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	let email = $state('');
	let password = $state('');
	let confirm = $state('');
	let acceptedTerms = $state(false);
	let submitting = $state(false);
	let error = $state('');
	let success = $state('');
	let showPassword = $state(false);

	// Password strength heuristic
	let strength = $derived(() => {
		if (!password) return { score: 0, label: '', tone: 'dim' };
		let s = 0;
		if (password.length >= 12) s += 1;
		if (password.length >= 16) s += 1;
		if (/[A-Z]/.test(password) && /[a-z]/.test(password)) s += 1;
		if (/\d/.test(password)) s += 1;
		if (/[^A-Za-z0-9]/.test(password)) s += 1;
		const labels = ['Too short', 'Weak', 'Fair', 'Strong', 'Excellent'];
		const tones  = ['crit',       'crit',  'warn', 'ok',      'ok'];
		const idx = Math.max(0, Math.min(4, s - 1));
		return { score: s, label: labels[idx], tone: tones[idx] };
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		success = '';

		if (!email || !password) { error = 'Email and password are required'; return; }
		if (password.length < 12) { error = 'Password must be at least 12 characters'; return; }
		if (password !== confirm) { error = 'Passwords do not match'; return; }
		if (!acceptedTerms)       { error = 'Please accept the terms to continue'; return; }

		submitting = true;
		try {
			await api.post('/v1/auth/register', { email, password });

			// Auto sign-in — skip the login detour
			try {
				const resp = await api.post<{ token?: string; requires_2fa?: boolean }>(
					'/v1/auth/sessions',
					{ email, password },
				);
				if (resp.token) {
					api.setToken(resp.token);
					window.location.href = '/';
					return;
				}
				// 2FA shouldn't trigger for a fresh account, but fall through just in case
			} catch {
				/* fall through — show manual-login success message */
			}

			success = 'Account created. Redirecting to sign-in…';
			setTimeout(() => { window.location.href = '/login'; }, 1200);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Registration failed';
		} finally {
			submitting = false;
		}
	}

	const CHAINS = ['bitcoin','ethereum','solana','tron','arbitrum','optimism','polygon','base','avalanche','bsc'];
</script>

<svelte:head>
	<title>Create account — MG Custody</title>
</svelte:head>

<div class="page">
	<!-- ═══════════ Brand ═══════════ -->
	<aside class="brand-panel">
		<div class="brand-grid" aria-hidden="true"></div>
		<div class="brand-glow" aria-hidden="true"></div>

		<div class="brand-content">
			<div class="brand-top">
				<span class="brand-mark">
					<svg width="36" height="36" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<defs>
							<linearGradient id="reg-g" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
								<stop stop-color="#6FA8FF"/>
								<stop offset="1" stop-color="#3D6BE0"/>
							</linearGradient>
						</defs>
						<path d="M16 3L4 8v9c0 7 4.8 13.2 12 15 7.2-1.8 12-8 12-15V8l-12-5z" fill="url(#reg-g)"/>
						<path d="M12 17l2.8 2.8L21 13.5" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</span>
				<div>
					<h1>MG Custody</h1>
					<span class="brand-sub">Self-hosted infrastructure</span>
				</div>
			</div>

			<div class="brand-hero">
				<h2 class="hero-title">Open a custody<br/>account.</h2>
				<p class="hero-desc">
					Every account gets deterministic deposit addresses across all supported networks.
					Your private keys never leave the custody infrastructure — you retain operational control.
				</p>
			</div>

			<ul class="feature-list">
				<li>
					<span class="feat-icon">{@html icon('key', 16)}</span>
					<div>
						<strong>Deterministic addresses</strong>
						<p>One address per chain, derived from your account ID. Regenerable from the master seed.</p>
					</div>
				</li>
				<li>
					<span class="feat-icon">{@html icon('radio', 16)}</span>
					<div>
						<strong>Live deposit detection</strong>
						<p>Scanners monitor all supported chains 24/7. No polling, no refresh.</p>
					</div>
				</li>
				<li>
					<span class="feat-icon">{@html icon('shield-check', 16)}</span>
					<div>
						<strong>Policy-controlled withdrawals</strong>
						<p>Circuit breaker, approval tiers, address whitelisting — applied automatically.</p>
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

	<!-- ═══════════ Form ═══════════ -->
	<section class="form-panel">
		<div class="form-container">

			<!-- Mobile mini-brand -->
			<div class="mobile-brand">
				<span class="brand-mark sm">
					<svg width="28" height="28" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<defs>
							<linearGradient id="reg-g2" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
								<stop stop-color="#6FA8FF"/>
								<stop offset="1" stop-color="#3D6BE0"/>
							</linearGradient>
						</defs>
						<path d="M16 3L4 8v9c0 7 4.8 13.2 12 15 7.2-1.8 12-8 12-15V8l-12-5z" fill="url(#reg-g2)"/>
						<path d="M12 17l2.8 2.8L21 13.5" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</span>
				<span>MG Custody</span>
			</div>

			<div class="form-header">
				<h2>Create your account</h2>
				<p>Free. Takes under 30 seconds.</p>
			</div>

			<form onsubmit={submit}>
				{#if error}
					<div class="banner banner-error">
						<span class="banner-icon">{@html icon('alert-triangle', 16)}</span>
						<span>{error}</span>
					</div>
				{/if}

				{#if success}
					<div class="banner banner-success">
						<span class="banner-icon">{@html icon('check', 16)}</span>
						<span>{success}</span>
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
							placeholder="you@company.com"
						/>
					</div>
				</div>

				<div class="field">
					<label for="password">Password</label>
					<div class="input-wrap">
						<span class="input-icon">{@html icon('lock', 15)}</span>
						<input
							id="password"
							type={showPassword ? 'text' : 'password'}
							bind:value={password}
							autocomplete="new-password"
							minlength="12"
							required
							placeholder="At least 12 characters"
						/>
						<button
							type="button"
							class="input-action"
							onclick={() => showPassword = !showPassword}
							aria-label={showPassword ? 'Hide password' : 'Show password'}
						>
							{@html icon(showPassword ? 'eye-off' : 'eye', 15)}
						</button>
					</div>
					{#if password}
						{@const s = strength()}
						<div class="strength-wrap">
							<div class="strength-bar">
								{#each [0,1,2,3,4] as i}
									<span
										class="strength-seg"
										class:filled={i < s.score}
										class:tone-crit={s.tone === 'crit'}
										class:tone-warn={s.tone === 'warn'}
										class:tone-ok={s.tone === 'ok'}
									></span>
								{/each}
							</div>
							<span class="strength-label tone-{s.tone}">{s.label}</span>
						</div>
					{/if}
				</div>

				<div class="field">
					<label for="confirm">Confirm password</label>
					<div class="input-wrap">
						<span class="input-icon">{@html icon('lock', 15)}</span>
						<input
							id="confirm"
							type={showPassword ? 'text' : 'password'}
							bind:value={confirm}
							autocomplete="new-password"
							required
							placeholder="Re-enter password"
						/>
					</div>
					{#if confirm && password}
						{#if confirm === password}
							<span class="field-hint tone-ok">
								{@html icon('check', 12)} Matches
							</span>
						{:else}
							<span class="field-hint tone-crit">
								{@html icon('x', 12)} Does not match
							</span>
						{/if}
					{/if}
				</div>

				<label class="checkbox-row">
					<input type="checkbox" bind:checked={acceptedTerms} />
					<span>
						I understand MG Custody holds my deposit addresses' private keys and
						agree to the operator's terms of service.
					</span>
				</label>

				<button type="submit" class="btn btn-primary full" disabled={submitting}>
					{#if submitting}
						<span class="spinner"></span>
						<span>Creating account…</span>
					{:else}
						<span>Create account</span>
						<span class="btn-chev">{@html icon('chevron-right', 14)}</span>
					{/if}
				</button>
			</form>

			<div class="divider"><span>Already have an account?</span></div>

			<a href="/login" class="btn btn-ghost full">
				<span>Sign in instead</span>
			</a>

			<footer class="form-footer">
				<span>MG Custody v1.0</span>
				<span class="dot-sep">·</span>
				<span>Self-hosted</span>
				<span class="dot-sep">·</span>
				<span>No vendor lock-in</span>
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

	/* Brand panel */
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

	.brand-content { position: relative; max-width: 480px; width: 100%; }

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
		width: 48px; height: 48px;
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
	.brand-sub { color: var(--fg-muted); font-size: 0.78rem; }

	.brand-hero { margin-bottom: var(--sp-8); }
	.hero-title {
		font-size: 2.1rem;
		font-weight: 650;
		letter-spacing: -0.03em;
		line-height: 1.15;
		color: var(--fg-strong);
		margin: 0 0 var(--sp-4);
	}
	.hero-desc { color: var(--fg-muted); font-size: 0.95rem; line-height: 1.55; margin: 0; }

	.feature-list {
		list-style: none;
		padding: 0;
		margin: 0 0 var(--sp-8);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}
	.feature-list li { display: flex; gap: var(--sp-3); align-items: flex-start; }
	.feat-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px; height: 32px;
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
	.feature-list p { margin: 0; color: var(--fg-muted); font-size: 0.82rem; line-height: 1.45; }

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
	.chain-strip-icons { display: flex; gap: 5px; opacity: 0.75; }
	.chain-strip-icons :global(svg) { border-radius: 50%; }

	/* Form panel */
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
		max-width: 400px;
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
	.form-header h2 {
		margin: 0 0 4px;
		font-size: 1.35rem;
		font-weight: 650;
		letter-spacing: -0.02em;
		color: var(--fg-strong);
	}
	.form-header p { margin: 0; color: var(--fg-muted); font-size: 0.88rem; }

	form { display: flex; flex-direction: column; gap: var(--sp-4); }

	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label {
		font-size: 0.78rem;
		font-weight: 500;
		color: var(--fg-muted);
	}

	.input-wrap { position: relative; display: flex; align-items: center; }
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
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.input-wrap input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}

	.strength-wrap {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-top: 4px;
	}
	.strength-bar { display: flex; gap: 3px; flex: 1; }
	.strength-seg {
		flex: 1;
		height: 3px;
		background: var(--surface-3);
		border-radius: 2px;
		transition: background 0.2s;
	}
	.strength-seg.filled.tone-crit { background: var(--red); }
	.strength-seg.filled.tone-warn { background: var(--yellow); }
	.strength-seg.filled.tone-ok { background: var(--green); }

	.strength-label {
		font-size: 0.72rem;
		font-weight: 500;
		min-width: 70px;
		text-align: right;
	}
	.tone-crit { color: var(--red); }
	.tone-warn { color: var(--yellow); }
	.tone-ok { color: var(--green); }

	.field-hint {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.74rem;
		font-weight: 500;
		margin-top: 2px;
	}

	.checkbox-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		font-size: 0.82rem;
		color: var(--fg-muted);
		cursor: pointer;
		line-height: 1.45;
	}
	.checkbox-row input {
		width: 15px;
		height: 15px;
		accent-color: var(--accent);
		cursor: pointer;
		margin-top: 2px;
		flex-shrink: 0;
	}

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
		color: var(--fg);
		border-color: var(--border);
	}
	.btn-ghost:hover { background: var(--hover-bg); border-color: var(--border-strong); }

	.btn-chev { display: inline-flex; }

	.spinner {
		width: 14px;
		height: 14px;
		border: 2px solid #0A0B0D;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

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
	}
	.banner-icon { display: inline-flex; flex-shrink: 0; }

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

	.form-footer {
		margin-top: var(--sp-6);
		text-align: center;
		font-size: 0.72rem;
		color: var(--fg-dim);
		display: flex;
		justify-content: center;
		gap: 8px;
	}
	.dot-sep { opacity: 0.5; }

	@media (max-width: 960px) {
		.page { flex-direction: column; }
		.brand-panel { display: none; }
		.form-panel { min-width: auto; padding: var(--sp-5); }
		.mobile-brand { display: flex; }
	}
</style>
