<script lang="ts">
	import { api } from '$lib/api';

	let email = $state('');
	let password = $state('');
	let submitting = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!email || !password) return;

		submitting = true;
		error = '';

		try {
			const resp = await api.post<{ token: string }>('/v1/auth/sessions', { email, password });
			api.setToken(resp.token);
			window.location.href = '/';
		} catch (err) {
			error = err instanceof Error ? err.message : 'Login failed';
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
</style>
