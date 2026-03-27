<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Vault {
		id: string;
		name: string;
		account_count: number;
		auto_fuel: boolean;
		created_at: string;
	}

	interface Account {
		id: string;
		chain: string;
		address: string;
		balance: string;
	}

	let vaults      = $state<Vault[]>([]);
	let loading     = $state(true);
	let error       = $state('');
	let showModal   = $state(false);
	let newName     = $state('');
	let newAutoFuel = $state(false);
	let creating    = $state(false);
	let createErr   = $state('');
	let selectedVault = $state<Vault | null>(null);
	let accounts    = $state<Account[]>([]);
	let acctLoading = $state(false);

	async function loadVaults() {
		loading = true; error = '';
		try {
			const data = await api.getJson<{ vaults: Vault[] }>('/v1/vaults');
			vaults = data.vaults ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load vaults';
		} finally { loading = false; }
	}

	async function createVault() {
		if (!newName.trim()) { createErr = 'Name is required'; return; }
		creating = true; createErr = '';
		try {
			await api.post('/v1/vaults', { name: newName.trim(), auto_fuel: newAutoFuel });
			showModal = false; newName = ''; newAutoFuel = false;
			await loadVaults();
		} catch (e) {
			createErr = e instanceof Error ? e.message : 'Failed to create vault';
		} finally { creating = false; }
	}

	async function openVault(v: Vault) {
		selectedVault = v; acctLoading = true; accounts = [];
		try {
			const data = await api.getJson<{ accounts: Account[] }>(`/v1/vaults/${v.id}/accounts`);
			accounts = data.accounts ?? [];
		} catch { accounts = []; }
		finally { acctLoading = false; }
	}

	onMount(loadVaults);

	function fmt(iso: string) { return new Date(iso).toLocaleDateString(); }
	function trunc(s: string) { return s.length > 20 ? s.slice(0, 10) + '…' + s.slice(-6) : s; }
</script>

<h1>Wallets</h1>

<div class="card">
	<div class="toolbar">
		<span>{vaults.length} vault{vaults.length !== 1 ? 's' : ''}</span>
		<button class="btn" onclick={() => { showModal = true; createErr = ''; }}>+ Create Vault</button>
	</div>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if vaults.length === 0}
		<p class="muted">No vaults found.</p>
	{:else}
		<table>
			<thead>
				<tr><th>Name</th><th>Accounts</th><th>Auto-fuel</th><th>Created</th></tr>
			</thead>
			<tbody>
				{#each vaults as v}
					<tr class="clickable" onclick={() => openVault(v)}>
						<td class="accent">{v.name}</td>
						<td>{v.account_count}</td>
						<td class={v.auto_fuel ? 'yes' : 'no'}>{v.auto_fuel ? 'On' : 'Off'}</td>
						<td class="muted">{fmt(v.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

{#if selectedVault}
	<div class="card" style="margin-top:1rem">
		<div class="toolbar">
			<span class="accent">{selectedVault.name}</span> — accounts
			<button class="btn-ghost" onclick={() => { selectedVault = null; accounts = []; }}>Close</button>
		</div>
		{#if acctLoading}
			<p class="muted">Loading accounts…</p>
		{:else if accounts.length === 0}
			<p class="muted">No accounts in this vault.</p>
		{:else}
			<table>
				<thead><tr><th>Chain</th><th>Address</th><th>Balance</th></tr></thead>
				<tbody>
					{#each accounts as a}
						<tr>
							<td>{a.chain}</td>
							<td class="mono muted" title={a.address}>{trunc(a.address)}</td>
							<td class="mono">{a.balance}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</div>
{/if}

{#if showModal}
	<div class="overlay" role="presentation" onclick={() => (showModal = false)} onkeydown={(e) => e.key === 'Escape' && (showModal = false)}>
		<div class="modal" role="dialog" aria-modal="true" aria-label="Create Vault" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
			<h2>Create Vault</h2>
			<label>
				Name
				<input bind:value={newName} placeholder="e.g. Hot Wallet" />
			</label>
			<label class="row">
				<input type="checkbox" bind:checked={newAutoFuel} />
				Auto-fuel accounts (Tron Energy)
			</label>
			{#if createErr}<p class="err">{createErr}</p>{/if}
			<div class="modal-actions">
				<button class="btn-ghost" onclick={() => (showModal = false)}>Cancel</button>
				<button class="btn" onclick={createVault} disabled={creating}>
					{creating ? 'Creating…' : 'Create'}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { margin: 0 0 1rem; font-size: 1.1rem; }

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
	}

	.btn {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}
	.btn:disabled { opacity: 0.6; cursor: not-allowed; }

	.btn-ghost {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg-muted, #8b949e);
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		cursor: pointer;
	}

	.clickable { cursor: pointer; }
	.clickable:hover td { background: var(--hover-bg, #1c2128); }

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.mono   { font-family: monospace; font-size: 0.85rem; }
	.muted  { color: var(--fg-muted, #8b949e); }
	.accent { color: var(--accent, #58a6ff); }
	.yes    { color: var(--green, #3fb950); }
	.no     { color: var(--fg-muted, #8b949e); }
	.err    { color: var(--red, #f85149); font-size: 0.85rem; margin: 0.4rem 0 0; }

	.overlay {
		position: fixed; inset: 0;
		background: rgba(0,0,0,0.6);
		display: flex; align-items: center; justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 10px;
		padding: 1.5rem;
		width: 360px;
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.82rem;
		color: var(--fg-muted, #8b949e);
	}

	label.row { flex-direction: row; align-items: center; color: var(--fg, #e1e4e8); }

	input:not([type="checkbox"]) {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		padding: 0.4rem 0.7rem;
		font-size: 0.9rem;
	}

	.modal-actions { display: flex; justify-content: flex-end; gap: 0.6rem; margin-top: 0.4rem; }
</style>
