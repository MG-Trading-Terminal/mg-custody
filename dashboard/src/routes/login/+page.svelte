<script lang="ts">
	import { api } from '$lib/api';

	let email = $state('');
	let password = $state('');
	let totpCode = $state('');
	let submitting = $state(false);
	let error = $state('');
	// 2FA flow: after password, server may return pending_token instead of session
	let pendingToken = $state('');
	let needs2fa = $state(false);

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
				// TOTP enabled — need second factor
				pendingToken = resp.pending_token;
				needs2fa = true;
			} else if (resp.token) {
				api.setToken(resp.token);
				window.location.href = '/';
			} else {
				error = 'Unexpected response from server';
			}
		} catch (err) {
			error = err instanceof Error ? err.message : 'Login failed';
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
</script>

<svelte:head>
	<title>Login — MG Custody</title>
</svelte:head>

<div class="page">
	<div class="card">
		<h1>MG Custody</h1>
		<p class="subtitle">Sign in to your account</p>

		{#if !needs2fa}
			<form onsubmit={submit}>
				{#if error}
					<p class="err">{error}</p>
				{/if}

				<label>
					Email
					<input type="email" bind:value={email} autocomplete="email" required />
				</label>

				<label>
					Password
					<input type="password" bind:value={password} autocomplete="current-password" required />
				</label>

				<button type="submit" disabled={submitting}>
					{submitting ? 'Signing in…' : 'Sign in'}
				</button>
			</form>
		{:else}
			<form onsubmit={verify2fa}>
				<p class="info">Enter the 6-digit code from your authenticator app</p>

				{#if error}
					<p class="err">{error}</p>
				{/if}

				<label>
					TOTP Code
					<input
						type="text"
						bind:value={totpCode}
						autocomplete="one-time-code"
						inputmode="numeric"
						pattern="[0-9]{6}"
						maxlength="6"
						placeholder="000000"
						required
					/>
				</label>

				<button type="submit" disabled={submitting}>
					{submitting ? 'Verifying…' : 'Verify'}
				</button>

				<button type="button" class="link-btn" onclick={() => { needs2fa = false; error = ''; }}>
					Back to login
				</button>
			</form>
		{/if}
	</div>
</div>

<style>
	.page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg, #0f1117);
	}

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 2rem;
		width: 100%;
		max-width: 380px;
	}

	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.3rem;
		color: var(--accent, #58a6ff);
		text-align: center;
	}

	.subtitle {
		text-align: center;
		color: var(--fg-muted, #8b949e);
		font-size: 0.85rem;
		margin: 0 0 1.5rem;
	}

	form { display: flex; flex-direction: column; gap: 1rem; }

	label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-size: 0.85rem;
		color: var(--fg-muted, #8b949e);
	}

	input {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		font-size: 0.9rem;
		padding: 0.5rem 0.75rem;
		outline: none;
	}

	input:focus { border-color: var(--accent, #58a6ff); }

	button {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 600;
		padding: 0.6rem;
		cursor: pointer;
		margin-top: 0.25rem;
	}

	button:disabled { opacity: 0.6; cursor: not-allowed; }

	.err {
		color: var(--red, #f85149);
		font-size: 0.85rem;
		margin: 0;
		padding: 0.5rem 0.75rem;
		background: rgba(248, 81, 73, 0.1);
		border-radius: 6px;
		border: 1px solid rgba(248, 81, 73, 0.3);
	}

	.info {
		color: var(--fg-muted, #8b949e);
		font-size: 0.85rem;
		margin: 0;
		text-align: center;
	}

	.link-btn {
		background: transparent;
		color: var(--accent, #58a6ff);
		border: none;
		font-size: 0.85rem;
		cursor: pointer;
		padding: 0.4rem;
	}

	.link-btn:hover { text-decoration: underline; }
</style>
