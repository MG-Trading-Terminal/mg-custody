<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Fido2Key {
		id: string;
		name: string;
		registered_at: string;
		last_used_at?: string;
	}

	interface Session {
		id: string;
		ip: string;
		user_agent: string;
		created_at: string;
		last_seen_at: string;
	}

	interface Profile {
		email: string;
		role: string;
		created_at: string;
		last_login_at: string;
		fido2_credentials: Fido2Key[];
		sessions: Session[];
	}

	let profile   = $state<Profile | null>(null);
	let loading   = $state(true);
	let error     = $state('');

	let pwCurrent = $state('');
	let pwNew     = $state('');
	let pwConfirm = $state('');
	let pwLoading = $state(false);
	let pwError   = $state('');
	let pwOk      = $state('');

	let revoking  = $state<string | null>(null);

	onMount(async () => {
		try {
			profile = await api.getJson<Profile>('/v1/user/profile');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load profile';
		} finally {
			loading = false;
		}
	});

	async function changePassword() {
		if (pwNew !== pwConfirm) { pwError = 'New passwords do not match'; return; }
		if (pwNew.length < 12)   { pwError = 'Password must be at least 12 characters'; return; }
		pwLoading = true;
		pwError   = '';
		pwOk      = '';
		try {
			await api.post('/v1/user/password', { current_password: pwCurrent, new_password: pwNew });
			pwOk      = 'Password updated successfully';
			pwCurrent = '';
			pwNew     = '';
			pwConfirm = '';
		} catch (e) {
			pwError = e instanceof Error ? e.message : 'Failed to update password';
		} finally {
			pwLoading = false;
		}
	}

	async function revokeSession(sessionId: string) {
		revoking = sessionId;
		try {
			await api.del(`/v1/user/sessions/${sessionId}`);
			if (profile) {
				profile = { ...profile, sessions: profile.sessions.filter(s => s.id !== sessionId) };
			}
		} catch {
			// session list will still reflect reality on next load
		} finally {
			revoking = null;
		}
	}

	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Profile</h1>

{#if loading}
	<p class="muted">Loading…</p>
{:else if error}
	<p class="err">{error}</p>
{:else if profile}
	<div class="card">
		<h3>Account</h3>
		<dl class="info-grid">
			<dt>Email</dt>     <dd>{profile.email}</dd>
			<dt>Role</dt>      <dd><span class="badge muted-bg">{profile.role}</span></dd>
			<dt>Member since</dt><dd>{fmt(profile.created_at)}</dd>
			<dt>Last login</dt><dd>{fmt(profile.last_login_at)}</dd>
			<dt>Security keys</dt><dd>{profile.fido2_credentials.length} registered</dd>
		</dl>
	</div>

	<div class="card">
		<h3>Change Password</h3>
		<div class="pw-form">
			<label class="field">
				Current Password
				<input type="password" bind:value={pwCurrent} />
			</label>
			<label class="field">
				New Password
				<input type="password" bind:value={pwNew} />
			</label>
			<label class="field">
				Confirm New Password
				<input type="password" bind:value={pwConfirm} />
			</label>
		</div>
		{#if pwError}<p class="err">{pwError}</p>{/if}
		{#if pwOk}<p class="ok">{pwOk}</p>{/if}
		<button class="btn-primary" onclick={changePassword} disabled={pwLoading}>
			{pwLoading ? 'Updating…' : 'Update Password'}
		</button>
	</div>

	<div class="card">
		<h3>Security Keys (FIDO2 / WebAuthn)</h3>
		{#if profile.fido2_credentials.length === 0}
			<p class="muted">No security keys registered.</p>
		{:else}
			<ul class="key-list">
				{#each profile.fido2_credentials as key}
					<li class="key-row">
						<div class="key-info">
							<span class="key-name">{key.name}</span>
							<span class="muted key-meta">
								Registered {fmt(key.registered_at)}
								{#if key.last_used_at}
									· Last used {fmt(key.last_used_at)}
								{/if}
							</span>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<div class="card">
		<h3>Active Sessions</h3>
		{#if profile.sessions.length === 0}
			<p class="muted">No active sessions.</p>
		{:else}
			<ul class="session-list">
				{#each profile.sessions as session}
					<li class="session-row">
						<div class="session-info">
							<span class="session-ip mono">{session.ip}</span>
							<span class="muted session-meta">{session.user_agent}</span>
							<span class="muted session-meta">
								Started {fmt(session.created_at)} · Last seen {fmt(session.last_seen_at)}
							</span>
						</div>
						<button
							class="btn-revoke"
							onclick={() => revokeSession(session.id)}
							disabled={revoking === session.id}
						>
							{revoking === session.id ? 'Revoking…' : 'Revoke'}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h3 { margin: 0 0 0.9rem; font-size: 0.9rem; color: var(--fg-muted, #8b949e); text-transform: uppercase; letter-spacing: 0.05em; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	.info-grid {
		display: grid; grid-template-columns: max-content 1fr;
		gap: 0.4rem 1.2rem; margin: 0; font-size: 0.9rem;
	}
	dt { color: var(--fg-muted, #8b949e); }
	dd { margin: 0; }

	.pw-form { display: flex; flex-direction: column; gap: 0.65rem; margin-bottom: 0.85rem; }
	.field { display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.8rem; color: var(--fg-muted, #8b949e); }

	input[type="password"] {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px; color: var(--fg, #e1e4e8);
		padding: 0.4rem 0.6rem; font-size: 0.9rem;
	}

	.btn-primary {
		background: var(--accent, #58a6ff); color: #0f1117;
		border: none; border-radius: 6px;
		padding: 0.5rem 1.1rem; font-size: 0.9rem; font-weight: 600; cursor: pointer;
	}
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-revoke {
		background: none; color: var(--red, #f85149);
		border: 1px solid rgba(248,81,73,.4);
		border-radius: 5px; padding: 0.25rem 0.65rem;
		font-size: 0.8rem; cursor: pointer; white-space: nowrap; flex-shrink: 0;
	}
	.btn-revoke:disabled { opacity: 0.5; cursor: not-allowed; }

	.key-list, .session-list { list-style: none; padding: 0; margin: 0; }

	.key-row, .session-row {
		display: flex; align-items: center; gap: 0.75rem;
		padding: 0.65rem 0; border-bottom: 1px solid var(--border, #30363d);
	}
	.key-row:last-child, .session-row:last-child { border-bottom: none; }

	.key-info, .session-info { display: flex; flex-direction: column; gap: 0.15rem; flex: 1; }
	.key-name   { font-weight: 600; font-size: 0.9rem; }
	.key-meta, .session-meta { font-size: 0.78rem; }
	.session-ip { font-size: 0.88rem; font-weight: 600; }

	.badge { border-radius: 4px; padding: 0.15rem 0.5rem; font-size: 0.78rem; text-transform: capitalize; font-weight: 600; }
	.muted-bg { background: rgba(139,148,158,.12); color: var(--fg-muted, #8b949e); }
	.muted  { color: var(--fg-muted, #8b949e); }
	.mono   { font-family: monospace; }
	.err    { color: var(--red, #f85149); font-size: 0.88rem; }
	.ok     { color: var(--green, #3fb950); font-size: 0.88rem; }
</style>
