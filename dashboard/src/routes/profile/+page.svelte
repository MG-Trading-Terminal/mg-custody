<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { icon } from '$lib/icons/ui';

	interface ProfileResp {
		user_id: string;
		email: string;
		role: string;
		fido2_credentials: number;
		created_at: string;
		last_login?: string | null;
	}

	interface Session {
		token_prefix?: string;
		id?: string;
		ip?: string;
		ip_address?: string;
		user_agent?: string;
		created_at: string;
		last_seen_at?: string;
	}

	let profile = $state<ProfileResp | null>(null);
	let loading = $state(true);
	let error = $state('');

	let sessions = $state<Session[]>([]);
	let sessionsLoading = $state(true);

	let pwCurrent = $state('');
	let pwNew = $state('');
	let pwConfirm = $state('');
	let pwLoading = $state(false);
	let pwError = $state('');
	let pwOk = $state('');
	let showPwNew = $state(false);

	let revoking = $state<string | null>(null);

	// ─── TOTP state ───────────────────────────────────────────────
	interface TotpSetupResponse {
		secret_base32: string;
		provisioning_uri: string;
		backup_codes: string[];
		message: string;
	}

	let totpEnabled = $state(false);
	let totpStage = $state<'idle' | 'setup' | 'verify' | 'enabled' | 'disabling'>('idle');
	let totpSetup = $state<TotpSetupResponse | null>(null);
	let totpCode = $state('');
	let totpLoading = $state(false);
	let totpError = $state('');
	let totpOk = $state('');
	let totpBackupAck = $state(false);
	let disableCode = $state('');

	onMount(() => {
		loadProfile();
		loadSessions();
	});

	async function startTotpSetup() {
		totpError = '';
		totpOk = '';
		totpLoading = true;
		try {
			totpSetup = await api.post<TotpSetupResponse>('/v1/auth/totp/setup');
			totpStage = 'setup';
			totpCode = '';
			totpBackupAck = false;
		} catch (e) {
			totpError = e instanceof Error ? e.message : 'Failed to start TOTP setup';
		} finally {
			totpLoading = false;
		}
	}

	async function verifyTotpSetup() {
		if (!/^\d{6}$/.test(totpCode)) {
			totpError = 'Enter the 6-digit code from your authenticator app';
			return;
		}
		totpError = '';
		totpLoading = true;
		try {
			await api.post('/v1/auth/totp/verify', { code: totpCode });
			totpOk = 'Two-factor authentication enabled.';
			totpStage = 'enabled';
			totpEnabled = true;
			totpCode = '';
			// Keep setup object so user can still scroll up to backup codes — but
			// require them to acknowledge, then clear.
		} catch (e) {
			totpError = e instanceof Error ? e.message : 'Code did not match';
			totpCode = '';
		} finally {
			totpLoading = false;
		}
	}

	async function disableTotp() {
		if (!/^\d{6}$/.test(disableCode) && disableCode.length < 8) {
			totpError = 'Enter a valid TOTP or backup code';
			return;
		}
		totpError = '';
		totpLoading = true;
		try {
			await api.post('/v1/auth/totp/disable', { code: disableCode });
			totpEnabled = false;
			totpStage = 'idle';
			totpSetup = null;
			disableCode = '';
			totpOk = 'Two-factor authentication disabled.';
		} catch (e) {
			totpError = e instanceof Error ? e.message : 'Unable to disable — code rejected';
			disableCode = '';
		} finally {
			totpLoading = false;
		}
	}

	function cancelSetup() {
		totpStage = 'idle';
		totpSetup = null;
		totpCode = '';
		totpError = '';
	}

	function copyText(text: string) {
		navigator.clipboard.writeText(text).catch(() => {});
	}

	function acknowledgeBackup() {
		totpBackupAck = true;
		totpStage = 'verify';
	}

	async function loadProfile() {
		loading = true;
		error = '';
		try {
			profile = await api.getJson<ProfileResp>('/v1/user/profile');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load profile';
		} finally {
			loading = false;
		}
	}

	async function loadSessions() {
		sessionsLoading = true;
		try {
			const resp = await api.getJson<{ sessions: Session[] } | Session[]>('/v1/auth/sessions');
			sessions = Array.isArray(resp) ? resp : (resp.sessions ?? []);
		} catch {
			sessions = [];
		} finally {
			sessionsLoading = false;
		}
	}

	async function changePassword() {
		pwError = '';
		pwOk = '';
		if (pwNew !== pwConfirm) { pwError = 'New passwords do not match'; return; }
		if (pwNew.length < 12) { pwError = 'Password must be at least 12 characters'; return; }

		pwLoading = true;
		try {
			await api.put('/v1/user/profile/password', {
				current_password: pwCurrent,
				new_password: pwNew,
			});
			pwOk = 'Password updated. Existing sessions have been revoked.';
			pwCurrent = '';
			pwNew = '';
			pwConfirm = '';
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to update password';
		} finally {
			pwLoading = false;
		}
	}

	async function revokeSession(id: string) {
		revoking = id;
		try {
			await api.del(`/v1/auth/sessions/${id}`);
			sessions = sessions.filter((s) => sessionKey(s) !== id);
		} catch {
			/* server will still reflect reality on reload */
		} finally {
			revoking = null;
		}
	}

	function sessionKey(s: Session): string {
		return s.token_prefix ?? s.id ?? '';
	}

	function fmtDate(iso?: string | null): string {
		if (!iso) return '—';
		return new Date(iso).toLocaleString(undefined, {
			year: 'numeric', month: 'short', day: 'numeric',
			hour: '2-digit', minute: '2-digit',
		});
	}

	function fmtRelative(iso?: string | null): string {
		if (!iso) return '—';
		const d = new Date(iso);
		const diff = Date.now() - d.getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1) return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		const dd = Math.floor(h / 24);
		if (dd < 7) return `${dd}d ago`;
		return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
	}

	function initials(email: string): string {
		const local = (email ?? '').split('@')[0] ?? '';
		const parts = local.split(/[._-]/).filter(Boolean);
		if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
		return local.slice(0, 2).toUpperCase() || '·';
	}
</script>

<div class="page-header">
	<div>
		<h1>Profile</h1>
		<p class="page-subtitle">Your account, security keys, and active sessions.</p>
	</div>
</div>

{#if loading}
	<div class="card skeleton" style="height: 180px"></div>
{:else if error}
	<div class="alert danger">
		<span class="alert-icon">{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn-ghost" onclick={loadProfile}>Retry</button>
	</div>
{:else if profile}
	<!-- Account summary -->
	<section class="card account">
		<div class="account-head">
			<span class="avatar-lg">{initials(profile.email)}</span>
			<div class="account-main">
				<span class="email">{profile.email}</span>
				<span class="role-line">
					<span class="role-chip">{profile.role}</span>
					<span class="muted">·</span>
					<span class="muted">Member since {fmtDate(profile.created_at)}</span>
				</span>
			</div>
			<div class="account-last">
				<span class="kpi-label">Last login</span>
				<span class="mono">{fmtRelative(profile.last_login)}</span>
			</div>
		</div>

		<div class="stat-row">
			<div class="stat">
				<span class="stat-icon">{@html icon('key', 16)}</span>
				<div>
					<span class="stat-label">Security keys</span>
					<span class="stat-value mono">{profile.fido2_credentials}</span>
				</div>
			</div>
			<div class="stat">
				<span class="stat-icon">{@html icon('shield-check', 16)}</span>
				<div>
					<span class="stat-label">2FA</span>
					<span class="stat-value">
						{#if totpEnabled}
							TOTP enabled
						{:else if profile.fido2_credentials > 0}
							FIDO2 enabled
						{:else}
							Password only
						{/if}
					</span>
				</div>
			</div>
			<div class="stat">
				<span class="stat-icon">{@html icon('activity', 16)}</span>
				<div>
					<span class="stat-label">Active sessions</span>
					<span class="stat-value mono">{sessionsLoading ? '…' : sessions.length}</span>
				</div>
			</div>
		</div>
	</section>

	<!-- TOTP / 2FA -->
	<section class="card">
		<header class="card-header">
			<div>
				<h2>Two-factor authentication (TOTP)</h2>
				<p class="card-sub">Required for Tier 2+ withdrawal approvals. Compatible with Google Authenticator, Authy, 1Password, or any RFC 6238 app.</p>
			</div>
			<span class="card-icon">{@html icon('shield', 16)}</span>
		</header>

		<div class="card-body">
			{#if totpError}
				<div class="alert danger inline">
					<span class="alert-icon">{@html icon('alert-triangle', 14)}</span>
					<span>{totpError}</span>
				</div>
			{/if}
			{#if totpOk && totpStage !== 'setup' && totpStage !== 'verify'}
				<div class="alert success inline">
					<span class="alert-icon">{@html icon('check', 14)}</span>
					<span>{totpOk}</span>
				</div>
			{/if}

			{#if totpStage === 'idle' && !totpEnabled}
				<div class="totp-idle">
					<div class="totp-state">
						<span class="state-dot dim"></span>
						<div>
							<span class="state-title">Not enabled</span>
							<span class="muted tiny">Your account is protected by password only.</span>
						</div>
					</div>
					<button class="btn btn-primary" onclick={startTotpSetup} disabled={totpLoading}>
						{#if totpLoading}
							<span class="spinner"></span><span>Starting…</span>
						{:else}
							<span>{@html icon('shield-check', 14)}</span>
							<span>Enable TOTP</span>
						{/if}
					</button>
				</div>

			{:else if totpStage === 'setup' && totpSetup}
				<!-- Step 1: show secret + backup codes -->
				<div class="totp-step">
					<span class="step-num">1</span>
					<div class="step-body">
						<h3>Add to your authenticator app</h3>
						<p class="muted">Open your authenticator and add a new account. Use the URI below or enter the secret manually.</p>

						<div class="secret-row">
							<div class="field">
								<label>Setup key (Base32)</label>
								<div class="copy-field">
									<code class="mono">{totpSetup.secret_base32}</code>
									<button class="icon-btn" onclick={() => copyText(totpSetup!.secret_base32)} title="Copy">
										{@html icon('copy', 14)}
									</button>
								</div>
							</div>
							<div class="field">
								<label>Provisioning URI</label>
								<div class="copy-field">
									<code class="mono uri-val">{totpSetup.provisioning_uri}</code>
									<button class="icon-btn" onclick={() => copyText(totpSetup!.provisioning_uri)} title="Copy">
										{@html icon('copy', 14)}
									</button>
								</div>
							</div>
						</div>
					</div>
				</div>

				<div class="totp-step">
					<span class="step-num">2</span>
					<div class="step-body">
						<h3>Save your backup codes</h3>
						<p class="muted">Each code works once if you lose access to your authenticator. Store them somewhere safe — they are shown <strong>only now</strong>.</p>

						<div class="backup-grid">
							{#each totpSetup.backup_codes as code, i}
								<div class="backup-code mono">
									<span class="bc-num">{i + 1}</span>
									<span>{code}</span>
								</div>
							{/each}
						</div>

						<div class="backup-actions">
							<button class="btn btn-ghost" onclick={() => copyText(totpSetup!.backup_codes.join('\n'))}>
								<span>{@html icon('copy', 14)}</span>
								<span>Copy all</span>
							</button>
							<label class="ack-row">
								<input type="checkbox" bind:checked={totpBackupAck} />
								<span>I have stored these codes in a safe place.</span>
							</label>
						</div>
					</div>
				</div>

				<div class="totp-actions">
					<button class="btn btn-ghost" onclick={cancelSetup}>Cancel</button>
					<button class="btn btn-primary" onclick={acknowledgeBackup} disabled={!totpBackupAck}>
						<span>Continue to verification</span>
						<span>{@html icon('chevron-right', 14)}</span>
					</button>
				</div>

			{:else if totpStage === 'verify'}
				<!-- Step 3: verify code -->
				<div class="totp-step">
					<span class="step-num">3</span>
					<div class="step-body">
						<h3>Confirm with a code</h3>
						<p class="muted">Enter the current 6-digit code from your authenticator to activate TOTP.</p>

						<div class="code-input-wrap">
							<input
								class="code-input mono"
								type="text"
								inputmode="numeric"
								pattern="[0-9]*"
								maxlength="6"
								bind:value={totpCode}
								placeholder="000000"
								autocomplete="one-time-code"
							/>
						</div>
					</div>
				</div>

				<div class="totp-actions">
					<button class="btn btn-ghost" onclick={cancelSetup}>Cancel</button>
					<button class="btn btn-primary" onclick={verifyTotpSetup} disabled={totpLoading || totpCode.length !== 6}>
						{#if totpLoading}
							<span class="spinner"></span><span>Activating…</span>
						{:else}
							<span>{@html icon('shield-check', 14)}</span>
							<span>Activate TOTP</span>
						{/if}
					</button>
				</div>

			{:else if totpStage === 'enabled' || totpEnabled}
				<!-- Enabled state -->
				<div class="totp-enabled">
					<div class="totp-state">
						<span class="state-dot ok"></span>
						<div>
							<span class="state-title">Active</span>
							<span class="muted tiny">Sign-in requires a code from your authenticator app.</span>
						</div>
					</div>

					<details class="disable-wrap">
						<summary>Disable TOTP</summary>
						<div class="disable-body">
							<p class="muted tiny">Enter a current 6-digit code or one of your 8+ character backup codes to confirm.</p>
							<div class="disable-row">
								<input
									class="code-input mono sm"
									type="text"
									bind:value={disableCode}
									placeholder="TOTP or backup code"
									maxlength="16"
								/>
								<button class="btn btn-danger-ghost" onclick={disableTotp} disabled={totpLoading || !disableCode}>
									{#if totpLoading}
										<span class="spinner small"></span><span>Working…</span>
									{:else}
										<span>Disable</span>
									{/if}
								</button>
							</div>
						</div>
					</details>
				</div>
			{/if}
		</div>
	</section>

	<div class="two-col">
		<!-- Password -->
		<section class="card">
			<header class="card-header">
				<div>
					<h2>Change password</h2>
					<p class="card-sub">Must be at least 12 characters. Sessions are revoked on change.</p>
				</div>
				<span class="card-icon">{@html icon('lock', 16)}</span>
			</header>

			<form class="card-body form-body" onsubmit={(e) => { e.preventDefault(); changePassword(); }}>
				{#if pwError}
					<div class="alert danger inline">
						<span class="alert-icon">{@html icon('alert-triangle', 14)}</span>
						<span>{pwError}</span>
					</div>
				{/if}
				{#if pwOk}
					<div class="alert success inline">
						<span class="alert-icon">{@html icon('check', 14)}</span>
						<span>{pwOk}</span>
					</div>
				{/if}

				<div class="field">
					<label for="pw-current">Current password</label>
					<div class="input-wrap">
						<span class="input-icon">{@html icon('lock', 14)}</span>
						<input id="pw-current" type="password" bind:value={pwCurrent} autocomplete="current-password" />
					</div>
				</div>

				<div class="field">
					<label for="pw-new">New password</label>
					<div class="input-wrap">
						<span class="input-icon">{@html icon('key', 14)}</span>
						<input
							id="pw-new"
							type={showPwNew ? 'text' : 'password'}
							bind:value={pwNew}
							autocomplete="new-password"
							minlength="12"
						/>
						<button
							type="button"
							class="input-action"
							onclick={() => showPwNew = !showPwNew}
							aria-label="Toggle password visibility"
						>
							{@html icon(showPwNew ? 'eye-off' : 'eye', 14)}
						</button>
					</div>
				</div>

				<div class="field">
					<label for="pw-confirm">Confirm new password</label>
					<div class="input-wrap">
						<span class="input-icon">{@html icon('key', 14)}</span>
						<input id="pw-confirm" type="password" bind:value={pwConfirm} autocomplete="new-password" />
					</div>
				</div>

				<button type="submit" class="btn btn-primary" disabled={pwLoading}>
					{#if pwLoading}
						<span class="spinner"></span>
						<span>Updating…</span>
					{:else}
						<span>Update password</span>
					{/if}
				</button>
			</form>
		</section>

		<!-- Security keys -->
		<section class="card">
			<header class="card-header">
				<div>
					<h2>Security keys</h2>
					<p class="card-sub">Hardware authenticators registered to this account.</p>
				</div>
				<span class="card-icon">{@html icon('shield-check', 16)}</span>
			</header>

			<div class="card-body">
				{#if profile.fido2_credentials === 0}
					<div class="empty-inline">
						<span class="empty-icon">{@html icon('key', 24)}</span>
						<p class="empty-title">No security keys registered</p>
						<p class="empty-sub">Add a hardware key to enable FIDO2 two-factor authentication.</p>
						<a href="/settings" class="btn btn-secondary">
							<span>Register a key</span>
							<span>{@html icon('chevron-right', 14)}</span>
						</a>
					</div>
				{:else}
					<div class="key-summary">
						<div class="key-count">
							<span class="mono key-count-num">{profile.fido2_credentials}</span>
							<span class="key-count-label">{profile.fido2_credentials === 1 ? 'key' : 'keys'} registered</span>
						</div>
						<a href="/settings" class="btn btn-ghost">Manage keys {@html icon('chevron-right', 14)}</a>
					</div>
				{/if}
			</div>
		</section>
	</div>

	<!-- Sessions -->
	<section class="card">
		<header class="card-header">
			<div>
				<h2>Active sessions</h2>
				<p class="card-sub">Devices currently signed in to your account.</p>
			</div>
			<span class="badge-count">{sessions.length}</span>
		</header>

		{#if sessionsLoading}
			<div class="card-body">
				<div class="skeleton-line"></div>
				<div class="skeleton-line"></div>
			</div>
		{:else if sessions.length === 0}
			<div class="empty-inline">
				<span class="empty-icon">{@html icon('activity', 24)}</span>
				<p class="empty-sub">No other active sessions.</p>
			</div>
		{:else}
			<ul class="session-list">
				{#each sessions as s}
					<li class="session-row">
						<span class="session-mark">{@html icon('activity', 14)}</span>
						<div class="session-main">
							<span class="session-ip mono">{s.ip_address ?? s.ip ?? 'unknown'}</span>
							<span class="session-ua">{s.user_agent ?? 'Unknown device'}</span>
						</div>
						<div class="session-meta">
							<span class="session-time">Started {fmtRelative(s.created_at)}</span>
							<span class="session-time muted">Active {fmtRelative(s.last_seen_at)}</span>
						</div>
						<button
							class="btn btn-danger-ghost"
							onclick={() => revokeSession(sessionKey(s))}
							disabled={revoking === sessionKey(s)}
						>
							{revoking === sessionKey(s) ? 'Revoking…' : 'Revoke'}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{/if}

<style>
	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-6);
	}

	h1 {
		margin: 0 0 4px;
		font-size: 1.5rem;
		font-weight: 650;
		letter-spacing: -0.022em;
	}

	.page-subtitle { margin: 0; color: var(--fg-muted); font-size: 0.88rem; }

	/* Card */
	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4);
		overflow: hidden;
	}

	.card-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}

	.card-header h2 {
		margin: 0 0 2px;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.card-sub {
		margin: 0;
		font-size: 0.8rem;
		color: var(--fg-muted);
	}

	.card-icon {
		color: var(--fg-dim);
		display: inline-flex;
		padding: 6px;
		background: var(--surface-2);
		border-radius: var(--r-sm);
	}

	.card-body {
		padding: var(--sp-5);
	}

	.badge-count {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 10px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	/* Account summary */
	.account { padding: 0; }

	.account-head {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-5);
		background:
			radial-gradient(circle at 0% 0%, var(--accent-soft) 0%, transparent 40%),
			var(--surface);
	}

	.avatar-lg {
		width: 56px;
		height: 56px;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--accent) 0%, #3D6BE0 100%);
		color: #0A0B0D;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 1.1rem;
		font-weight: 700;
		flex-shrink: 0;
	}

	.account-main { flex: 1; display: flex; flex-direction: column; gap: 4px; min-width: 0; }

	.email {
		font-size: 1.05rem;
		font-weight: 600;
		color: var(--fg-strong);
		letter-spacing: -0.01em;
	}

	.role-line {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 0.8rem;
	}

	.role-chip {
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--accent);
		background: var(--accent-soft);
		padding: 2px 8px;
		border-radius: var(--r-full);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		border: 1px solid var(--accent-line);
	}

	.muted { color: var(--fg-muted); }

	.account-last {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 4px;
		text-align: right;
		flex-shrink: 0;
	}

	.kpi-label {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-muted);
	}

	.stat-row {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		border-top: 1px solid var(--border);
	}

	.stat {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-right: 1px solid var(--border);
	}

	.stat:last-child { border-right: none; }

	.stat > div { display: flex; flex-direction: column; gap: 2px; }

	.stat-icon {
		color: var(--accent);
		background: var(--accent-soft);
		padding: 7px;
		border-radius: var(--r-sm);
		display: inline-flex;
	}

	.stat-label {
		font-size: 0.7rem;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-weight: 600;
	}

	.stat-value {
		font-size: 1rem;
		font-weight: 600;
		color: var(--fg-strong);
		letter-spacing: -0.01em;
	}

	/* Two-column grid */
	.two-col {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--sp-4);
		margin-bottom: var(--sp-4);
	}

	/* Form body */
	.form-body { display: flex; flex-direction: column; gap: var(--sp-3); }

	.field { display: flex; flex-direction: column; gap: 5px; }

	.field label {
		font-size: 0.76rem;
		font-weight: 500;
		color: var(--fg-muted);
	}

	.input-wrap { position: relative; display: flex; align-items: center; }

	.input-icon {
		position: absolute;
		left: 10px;
		color: var(--fg-dim);
		display: inline-flex;
		pointer-events: none;
	}

	.input-action {
		position: absolute;
		right: 6px;
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border-radius: 4px;
	}
	.input-action:hover { color: var(--fg); background: var(--hover-bg); }

	.input-wrap input {
		flex: 1;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.88rem;
		padding: 9px 12px 9px 32px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s, background 0.15s;
	}

	.input-wrap input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		padding: 8px 13px;
		font-size: 0.85rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		text-decoration: none;
		transition: background 0.15s, border-color 0.15s;
	}
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.55; cursor: not-allowed; }

	.btn-primary {
		background: var(--accent);
		color: #0A0B0D;
		border-color: var(--accent);
		font-weight: 600;
		align-self: flex-start;
	}
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); }

	.btn-secondary { background: var(--surface-2); }

	.btn-ghost {
		background: transparent;
		border-color: transparent;
		color: var(--fg-muted);
	}
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.btn-danger-ghost {
		background: transparent;
		border-color: transparent;
		color: var(--red);
	}
	.btn-danger-ghost:hover:not(:disabled) {
		background: var(--red-soft);
		border-color: rgba(248, 113, 113, 0.25);
	}

	.spinner {
		width: 12px;
		height: 12px;
		border: 2px solid #0A0B0D;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* Alert */
	.alert {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
	}
	.alert.inline { padding: 8px 10px; font-size: 0.8rem; }
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.alert.success {
		background: var(--green-soft);
		color: var(--green);
		border: 1px solid rgba(52, 211, 153, 0.25);
	}
	.alert-icon { display: inline-flex; flex-shrink: 0; }
	.alert .btn-ghost { margin-left: auto; padding: 4px 8px; font-size: 0.78rem; }

	/* Keys summary */
	.key-summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.key-count { display: flex; align-items: baseline; gap: 8px; }

	.key-count-num {
		font-size: 2rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
	}

	.key-count-label { color: var(--fg-muted); font-size: 0.88rem; }

	/* Empty state */
	.empty-inline {
		padding: var(--sp-6);
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}

	.empty-icon {
		color: var(--fg-dim);
		background: var(--surface-2);
		padding: 14px;
		border-radius: var(--r-md);
		display: inline-flex;
		margin-bottom: 4px;
	}

	.empty-title {
		margin: 0;
		font-weight: 600;
		color: var(--fg);
		font-size: 0.95rem;
	}

	.empty-sub { margin: 0; color: var(--fg-muted); font-size: 0.85rem; }

	.empty-inline .btn { margin-top: var(--sp-3); }

	/* Sessions */
	.session-list { list-style: none; margin: 0; padding: 0; }

	.session-row {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.session-row:last-child { border-bottom: none; }
	.session-row:hover { background: var(--hover-bg); }

	.session-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border-radius: var(--r-sm);
		background: var(--accent-soft);
		color: var(--accent);
		flex-shrink: 0;
	}

	.session-main { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.session-ip { font-size: 0.88rem; color: var(--fg-strong); font-weight: 500; }
	.session-ua { font-size: 0.78rem; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }

	.session-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 1px; flex-shrink: 0; }
	.session-time { font-size: 0.76rem; color: var(--fg-muted); }

	/* Skeletons */
	.skeleton {
		background: linear-gradient(90deg, var(--surface) 0%, var(--surface-2) 50%, var(--surface) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
	}
	.skeleton-line {
		height: 18px;
		margin-bottom: 8px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
	}
	@keyframes shimmer {
		0%   { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}

	@media (max-width: 900px) {
		.two-col { grid-template-columns: 1fr; }
		.stat-row { grid-template-columns: 1fr; }
		.stat { border-right: none; border-bottom: 1px solid var(--border); }
		.stat:last-child { border-bottom: none; }
		.account-head { flex-wrap: wrap; }
		.account-last { width: 100%; align-items: flex-start; border-top: 1px solid var(--border); padding-top: var(--sp-3); margin-top: var(--sp-2); }
		.session-row { flex-wrap: wrap; }
		.session-meta { align-items: flex-start; width: 100%; flex-direction: row; gap: 12px; }
	}

	/* ── TOTP section ─────────────────────────────────────────── */
	.totp-idle, .totp-enabled {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		flex-wrap: wrap;
	}
	.totp-state { display: flex; align-items: center; gap: 12px; }
	.totp-state > div { display: flex; flex-direction: column; gap: 2px; }
	.state-dot {
		width: 10px; height: 10px;
		border-radius: 50%;
		background: var(--fg-dim);
		flex-shrink: 0;
	}
	.state-dot.dim { background: var(--fg-dim); }
	.state-dot.ok { background: var(--green); box-shadow: 0 0 0 3px rgba(52, 211, 153, 0.18); }
	.state-title { font-size: 0.95rem; font-weight: 600; color: var(--fg-strong); }

	.totp-step {
		display: flex;
		gap: var(--sp-3);
		margin-bottom: var(--sp-5);
		padding-bottom: var(--sp-4);
		border-bottom: 1px solid var(--border);
	}
	.totp-step:last-of-type { border-bottom: none; padding-bottom: 0; }

	.step-num {
		flex-shrink: 0;
		width: 28px; height: 28px;
		border-radius: 50%;
		background: var(--accent-soft);
		color: var(--accent);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 0.82rem;
		font-weight: 700;
		margin-top: 2px;
	}
	.step-body { flex: 1; min-width: 0; }
	.step-body h3 {
		margin: 0 0 4px;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--fg-strong);
	}
	.step-body p { margin: 0 0 var(--sp-3); font-size: 0.85rem; line-height: 1.5; }

	.secret-row {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}
	.copy-field {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 10px 10px 12px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.copy-field code {
		flex: 1;
		font-size: 0.82rem;
		color: var(--fg-strong);
		word-break: break-all;
	}
	.uri-val {
		font-size: 0.76rem !important;
		line-height: 1.5;
		max-height: 60px;
		overflow-y: auto;
	}

	.backup-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 6px;
		margin-bottom: var(--sp-3);
	}
	.backup-code {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		background: var(--yellow-soft);
		border: 1px solid rgba(245, 185, 66, 0.25);
		border-radius: var(--r-sm);
		font-size: 0.88rem;
		color: var(--fg-strong);
		font-weight: 500;
		letter-spacing: 0.02em;
	}
	.bc-num {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--yellow);
		min-width: 18px;
	}

	.backup-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
		flex-wrap: wrap;
	}

	.ack-row {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 0.82rem;
		color: var(--fg);
		cursor: pointer;
	}
	.ack-row input {
		width: 15px; height: 15px;
		accent-color: var(--accent);
	}

	.code-input-wrap {
		display: flex;
		justify-content: center;
	}
	.code-input {
		width: 240px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg-strong);
		font-size: 1.6rem;
		text-align: center;
		letter-spacing: 0.45rem;
		padding: 14px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.code-input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
	}
	.code-input.sm {
		width: auto;
		flex: 1;
		font-size: 0.9rem;
		letter-spacing: 0.08em;
		text-align: left;
		padding: 9px 12px;
	}

	.totp-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--sp-2);
		margin-top: var(--sp-4);
		padding-top: var(--sp-3);
		border-top: 1px solid var(--border);
	}

	.disable-wrap {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		overflow: hidden;
	}
	.disable-wrap summary {
		padding: 10px 14px;
		cursor: pointer;
		font-size: 0.85rem;
		color: var(--fg-muted);
		user-select: none;
	}
	.disable-wrap summary:hover { color: var(--fg); }
	.disable-wrap[open] summary { border-bottom: 1px solid var(--border); color: var(--fg); }
	.disable-body { padding: var(--sp-3) var(--sp-4); display: flex; flex-direction: column; gap: var(--sp-3); }
	.disable-row { display: flex; gap: 8px; align-items: center; }

	.spinner.small { width: 10px; height: 10px; border-width: 1.5px; }

	@media (max-width: 600px) {
		.backup-grid { grid-template-columns: 1fr; }
		.totp-idle, .totp-enabled { flex-direction: column; align-items: stretch; }
	}
</style>
