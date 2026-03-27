<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Address {
		address: string;
		chain: string;
		user_id: string;
		whitelisted: boolean;
		created_at: string;
	}

	const CHAINS = ['ethereum', 'bitcoin', 'solana', 'tron'];

	let addresses  = $state<Address[]>([]);
	let loading    = $state(true);
	let error      = $state('');
	let chain      = $state('ethereum');
	let copied     = $state('');

	async function load(c: string) {
		chain = c; loading = true; error = '';
		try {
			const data = await api.getJson<{ addresses: Address[] }>(`/v1/addresses/chain/${c}`);
			addresses = data.addresses ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load addresses';
		} finally { loading = false; }
	}

	async function copy(addr: string) {
		try {
			await navigator.clipboard.writeText(addr);
			copied = addr;
			setTimeout(() => { if (copied === addr) copied = ''; }, 1800);
		} catch { /* clipboard not available */ }
	}

	onMount(() => load('ethereum'));

	function trunc(s: string) { return s.length > 20 ? s.slice(0, 10) + '…' + s.slice(-6) : s; }
	function fmt(iso: string)  { return new Date(iso).toLocaleDateString(); }
</script>

<h1>Addresses</h1>

<div class="card">
	<div class="toolbar">
		<div class="tabs">
			{#each CHAINS as c}
				<button class="tab" class:active={chain === c} onclick={() => load(c)}>
					{c}
				</button>
			{/each}
		</div>
		<span class="muted">{addresses.length} address{addresses.length !== 1 ? 'es' : ''}</span>
	</div>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if addresses.length === 0}
		<p class="muted">No addresses on {chain}.</p>
	{:else}
		<table>
			<thead>
				<tr><th>Address</th><th>User ID</th><th>Whitelist</th><th>Created</th><th></th></tr>
			</thead>
			<tbody>
				{#each addresses as a}
					<tr>
						<td class="mono" title={a.address}>{trunc(a.address)}</td>
						<td class="muted">{a.user_id}</td>
						<td>
							{#if a.whitelisted}
								<span class="badge green">Whitelisted</span>
							{:else}
								<span class="muted">—</span>
							{/if}
						</td>
						<td class="muted">{fmt(a.created_at)}</td>
						<td>
							<button class="copy-btn" onclick={() => copy(a.address)}>
								{copied === a.address ? 'Copied' : 'Copy'}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1rem;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.tabs { display: flex; gap: 0.4rem; }

	.tab {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg-muted, #8b949e);
		padding: 0.3rem 0.75rem;
		font-size: 0.8rem;
		cursor: pointer;
		text-transform: capitalize;
	}

	.tab.active {
		border-color: var(--accent, #58a6ff);
		color: var(--accent, #58a6ff);
	}

	.copy-btn {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 4px;
		color: var(--fg-muted, #8b949e);
		padding: 0.15rem 0.55rem;
		font-size: 0.75rem;
		cursor: pointer;
	}
	.copy-btn:hover { border-color: var(--accent, #58a6ff); color: var(--accent, #58a6ff); }

	.badge { border-radius: 4px; padding: 0.15rem 0.5rem; font-size: 0.75rem; font-weight: 600; }
	.green { background: rgba(63,185,80,.12); color: var(--green, #3fb950); }

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.mono  { font-family: monospace; font-size: 0.85rem; }
	.muted { color: var(--fg-muted, #8b949e); }
	.err   { color: var(--red, #f85149); }
</style>
