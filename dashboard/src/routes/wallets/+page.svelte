<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	interface Vault {
		id: string;
		name: string;
		account_count: number;
		auto_fuel: boolean;
		created_at: string;
		tier?: 'hot' | 'warm' | 'cold';
		total_usd?: string;
	}

	interface Account {
		id: string;
		chain: string;
		address: string;
		balance: string;
		asset?: string;
		usd_value?: string;
	}

	let vaults = $state<Vault[]>([]);
	let loading = $state(true);
	let error = $state('');

	let showModal = $state(false);
	let newName = $state('');
	let newAutoFuel = $state(false);
	let newTier = $state<'hot' | 'warm' | 'cold'>('hot');
	let creating = $state(false);
	let createErr = $state('');

	let selectedVault = $state<Vault | null>(null);
	let accounts = $state<Account[]>([]);
	let acctLoading = $state(false);

	async function loadVaults() {
		loading = true;
		error = '';
		try {
			const data = await api.getJson<{ vaults: Vault[] }>('/v1/vaults');
			vaults = data.vaults ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load vaults';
		} finally {
			loading = false;
		}
	}

	async function createVault() {
		if (!newName.trim()) { createErr = 'Name is required'; return; }
		creating = true;
		createErr = '';
		try {
			await api.post('/v1/vaults', { name: newName.trim(), auto_fuel: newAutoFuel, tier: newTier });
			showModal = false;
			newName = '';
			newAutoFuel = false;
			newTier = 'hot';
			await loadVaults();
		} catch (e) {
			createErr = e instanceof Error ? e.message : 'Failed to create vault';
		} finally {
			creating = false;
		}
	}

	async function openVault(v: Vault) {
		selectedVault = v;
		acctLoading = true;
		accounts = [];
		try {
			const data = await api.getJson<{ accounts: Account[] }>(`/v1/vaults/${v.id}/accounts`);
			accounts = data.accounts ?? [];
		} catch { accounts = []; }
		finally { acctLoading = false; }
	}

	onMount(loadVaults);

	function fmtDate(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
	}

	function trunc(s: string, pre = 10, post = 6): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function fmtUsd(v?: string): string {
		if (!v) return '—';
		const n = Number(v);
		if (!isFinite(n)) return '—';
		if (n >= 1_000_000) return `$${(n/1_000_000).toFixed(2)}M`;
		if (n >= 1_000) return `$${(n/1_000).toFixed(1)}K`;
		return `$${n.toFixed(2)}`;
	}

	function tierTone(t?: string): 'hot' | 'warm' | 'cold' {
		if (t === 'warm') return 'warm';
		if (t === 'cold') return 'cold';
		return 'hot';
	}

	function tierLabel(t?: string): string {
		return t ? t.charAt(0).toUpperCase() + t.slice(1) : 'Hot';
	}

	function copy(s: string) { navigator.clipboard.writeText(s); }

	// Aggregate stats
	let stats = $derived({
		total: vaults.length,
		accounts: vaults.reduce((a, v) => a + (v.account_count || 0), 0),
		hot: vaults.filter(v => (v.tier ?? 'hot') === 'hot').length,
		warm: vaults.filter(v => v.tier === 'warm').length,
		cold: vaults.filter(v => v.tier === 'cold').length,
	});
</script>

<!-- ═══════════ Header ═══════════ -->
<div class="page-head">
	<div>
		<h1>Wallets</h1>
		<p class="page-sub">Vaults are logical groupings of addresses. Hot/warm/cold tiering controls fund segregation.</p>
	</div>
	<button class="btn btn-primary" onclick={() => { showModal = true; createErr = ''; }}>
		<span>{@html icon('plus', 14)}</span>
		<span>New vault</span>
	</button>
</div>

<!-- ═══════════ Stats ═══════════ -->
<section class="stat-strip">
	<div class="stat-tile">
		<span class="stat-label">Vaults</span>
		<span class="stat-value mono">{stats.total}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Accounts</span>
		<span class="stat-value mono">{stats.accounts}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Hot tier</span>
		<span class="stat-value mono tone-hot">{stats.hot}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Warm tier</span>
		<span class="stat-value mono tone-warm">{stats.warm}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Cold tier</span>
		<span class="stat-value mono tone-cold">{stats.cold}</span>
	</div>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={loadVaults}>Retry</button>
	</div>
{/if}

<!-- ═══════════ Vaults list ═══════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Vaults</h2>
			<p class="panel-sub">Click to see accounts and balances</p>
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			<div class="skel-line"></div>
			<div class="skel-line"></div>
			<div class="skel-line"></div>
		</div>
	{:else if vaults.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('wallets', 22)}</span>
			<div>
				<p class="empty-title">No vaults yet</p>
				<p class="muted">Create your first vault to group addresses by purpose (trading, operations, cold storage).</p>
			</div>
			<button class="btn btn-primary" onclick={() => { showModal = true; createErr = ''; }}>
				<span>{@html icon('plus', 14)}</span>
				<span>Create vault</span>
			</button>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>Vault</th>
					<th>Tier</th>
					<th class="num">Accounts</th>
					<th class="num">Balance</th>
					<th>Auto-fuel</th>
					<th class="num">Created</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each vaults as v}
					<tr class="clickable" onclick={() => openVault(v)}>
						<td>
							<div class="vault-cell">
								<span class="vault-mark tone-{tierTone(v.tier)}">{@html icon('wallets', 14)}</span>
								<div>
									<span class="vault-name">{v.name}</span>
									<span class="vault-id mono muted">{trunc(v.id, 8, 4)}</span>
								</div>
							</div>
						</td>
						<td><span class="tier-pill tone-{tierTone(v.tier)}">{tierLabel(v.tier)}</span></td>
						<td class="num mono">{v.account_count}</td>
						<td class="num mono strong">{fmtUsd(v.total_usd)}</td>
						<td>
							{#if v.auto_fuel}
								<span class="chip tone-ok">
									<span class="chip-dot"></span>
									Enabled
								</span>
							{:else}
								<span class="chip tone-muted">Disabled</span>
							{/if}
						</td>
						<td class="num mono muted">{fmtDate(v.created_at)}</td>
						<td>
							<button class="row-action" aria-label="View">{@html icon('chevron-right', 14)}</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<!-- ═══════════ Vault drawer ═══════════ -->
{#if selectedVault}
	<div class="drawer-backdrop" role="presentation" onclick={() => { selectedVault = null; accounts = []; }} onkeydown={(e) => e.key === 'Escape' && (selectedVault = null)}></div>
	<aside class="drawer">
		<header class="drawer-head">
			<div class="drawer-title">
				<span class="vault-mark lg tone-{tierTone(selectedVault.tier)}">{@html icon('wallets', 18)}</span>
				<div>
					<h3>{selectedVault.name}</h3>
					<span class="mono muted">{trunc(selectedVault.id, 14, 8)}</span>
				</div>
			</div>
			<button class="icon-btn" onclick={() => { selectedVault = null; accounts = []; }}>{@html icon('x', 16)}</button>
		</header>

		<div class="drawer-meta">
			<div><span class="meta-label">Tier</span><span class="tier-pill tone-{tierTone(selectedVault.tier)}">{tierLabel(selectedVault.tier)}</span></div>
			<div><span class="meta-label">Accounts</span><span class="mono">{selectedVault.account_count}</span></div>
			<div><span class="meta-label">Auto-fuel</span><span>{selectedVault.auto_fuel ? 'Enabled' : 'Disabled'}</span></div>
		</div>

		<div class="drawer-body">
			<div class="section-title">
				<span>Accounts</span>
				<span class="badge-count">{accounts.length}</span>
			</div>

			{#if acctLoading}
				<div class="skel-line"></div>
				<div class="skel-line"></div>
			{:else if accounts.length === 0}
				<div class="empty-inline">
					<p class="muted">No accounts in this vault yet.</p>
				</div>
			{:else}
				<ul class="account-list">
					{#each accounts as a}
						<li class="account-row">
							<span class="acct-chain">
								{@html chainIcon(a.chain, 16)}
								<span class="capitalize">{a.chain}</span>
							</span>
							<div class="acct-addr">
								<code class="mono">{trunc(a.address, 12, 8)}</code>
								<button class="copy-mini" onclick={() => copy(a.address)} aria-label="Copy">
									{@html icon('copy', 12)}
								</button>
							</div>
							<div class="acct-balance">
								<span class="mono strong">{a.balance}</span>
								<span class="mono muted tiny">{a.asset ?? ''}</span>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</aside>
{/if}

<!-- ═══════════ Create modal ═══════════ -->
{#if showModal}
	<div class="overlay" role="presentation" onclick={() => showModal = false} onkeydown={(e) => e.key === 'Escape' && (showModal = false)}>
		<div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
			<header class="modal-head">
				<h3>Create vault</h3>
				<button class="icon-btn" onclick={() => showModal = false}>{@html icon('x', 16)}</button>
			</header>

			<div class="modal-body">
				<div class="field">
					<label for="v-name">Name</label>
					<input id="v-name" bind:value={newName} placeholder="e.g. Trading Hot" />
				</div>

				<div class="field">
					<label>Tier</label>
					<div class="tier-picker">
						{#each ['hot','warm','cold'] as t}
							<button
								class="tier-option tone-{t}"
								class:active={newTier === t}
								onclick={() => newTier = t as 'hot'|'warm'|'cold'}
								type="button"
							>
								<span class="tier-dot"></span>
								<span class="capitalize">{t}</span>
								<span class="tier-hint">
									{t === 'hot' ? '≤5% AUM' : t === 'warm' ? '≤15% AUM' : '≥80% AUM'}
								</span>
							</button>
						{/each}
					</div>
				</div>

				<label class="check-row">
					<input type="checkbox" bind:checked={newAutoFuel} />
					<div>
						<span class="check-label">Auto-fuel accounts</span>
						<span class="muted tiny">Automatically top up Tron Energy / ETH gas from gas station</span>
					</div>
				</label>

				{#if createErr}
					<div class="alert danger inline">
						<span>{@html icon('alert-triangle', 14)}</span>
						<span>{createErr}</span>
					</div>
				{/if}
			</div>

			<div class="modal-actions">
				<button class="btn btn-ghost" onclick={() => showModal = false}>Cancel</button>
				<button class="btn btn-primary" onclick={createVault} disabled={creating}>
					{#if creating}<span class="spinner"></span><span>Creating…</span>{:else}<span>Create vault</span>{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	/* ═══ Head ═══ */
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
		flex-wrap: wrap;
	}
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	/* ═══ Stats ═══ */
	.stat-strip {
		display: grid;
		grid-template-columns: repeat(5, 1fr);
		gap: 1px;
		background: var(--border);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
	}
	.stat-tile {
		padding: var(--sp-4) var(--sp-5);
		background: var(--surface);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.stat-label {
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-muted);
	}
	.stat-value {
		font-size: 1.4rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
		line-height: 1;
	}
	.stat-value.tone-hot { color: var(--red); }
	.stat-value.tone-warm { color: var(--yellow); }
	.stat-value.tone-cold { color: var(--blue); }

	/* ═══ Alert ═══ */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.inline { padding: 8px 10px; font-size: 0.8rem; margin: 0; }
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.alert .btn { margin-left: auto; }

	/* ═══ Panel ═══ */
	.panel {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4);
		overflow: hidden;
	}
	.panel-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	/* ═══ Buttons ═══ */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 14px;
		font-size: 0.86rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: #0A0B0D; border-color: var(--accent); font-weight: 600; }
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.icon-btn {
		display: inline-flex; align-items: center; justify-content: center;
		width: 28px; height: 28px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		cursor: pointer;
	}
	.icon-btn:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.spinner {
		width: 13px; height: 13px;
		border: 2px solid #0A0B0D;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ═══ Data table ═══ */
	.data-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	.data-table thead th {
		text-align: left;
		padding: var(--sp-3) var(--sp-5);
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
	}
	.data-table tbody td {
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr.clickable { cursor: pointer; transition: background 0.15s; }
	.data-table tbody tr.clickable:hover { background: var(--hover-bg); }
	.data-table th.num, .data-table td.num { text-align: right; }

	.vault-cell { display: flex; align-items: center; gap: 10px; }
	.vault-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px; height: 30px;
		border-radius: var(--r-sm);
		flex-shrink: 0;
	}
	.vault-mark.lg { width: 42px; height: 42px; }
	.vault-mark.tone-hot { background: var(--red-soft); color: var(--red); }
	.vault-mark.tone-warm { background: var(--yellow-soft); color: var(--yellow); }
	.vault-mark.tone-cold { background: var(--blue-soft); color: var(--blue); }

	.vault-cell > div { display: flex; flex-direction: column; gap: 2px; line-height: 1.2; }
	.vault-name { font-weight: 600; color: var(--fg-strong); }
	.vault-id { font-size: 0.72rem; }

	.tier-pill {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 600;
		padding: 2px 9px;
		border-radius: var(--r-full);
		text-transform: capitalize;
	}
	.tier-pill.tone-hot { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.3); }
	.tier-pill.tone-warm { background: var(--yellow-soft); color: var(--yellow); border: 1px solid rgba(245, 185, 66, 0.3); }
	.tier-pill.tone-cold { background: var(--blue-soft); color: var(--blue); border: 1px solid rgba(96, 165, 250, 0.3); }

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 500;
		padding: 2px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-muted { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	.muted { color: var(--fg-muted); }
	.strong { color: var(--fg-strong); font-weight: 500; }
	.capitalize { text-transform: capitalize; }

	.row-action {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px; height: 24px;
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		border-radius: var(--r-sm);
	}
	tr:hover .row-action { color: var(--fg); background: var(--surface-2); }

	/* ═══ Empty & skel ═══ */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-6);
	}
	.empty-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 44px; height: 44px;
		background: var(--accent-soft);
		color: var(--accent);
		border-radius: var(--r-md);
		flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.empty-inline { text-align: center; padding: var(--sp-5); }

	.skel-line {
		height: 44px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 8px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	/* ═══ Drawer ═══ */
	.drawer-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(4px);
		z-index: 80;
	}
	.drawer {
		position: fixed;
		top: 0; right: 0;
		width: 460px;
		max-width: 100vw;
		height: 100vh;
		background: var(--surface);
		border-left: 1px solid var(--border);
		box-shadow: -10px 0 40px rgba(0, 0, 0, 0.4);
		z-index: 90;
		display: flex;
		flex-direction: column;
		animation: slideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
	}
	@keyframes slideIn { from { transform: translateX(100%); } to { transform: translateX(0); } }

	.drawer-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.drawer-title { display: flex; align-items: center; gap: 12px; }
	.drawer-title h3 { margin: 0 0 2px; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }
	.drawer-title .muted { font-size: 0.76rem; }

	.drawer-meta {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
	}
	.drawer-meta > div {
		padding: var(--sp-3) var(--sp-4);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.drawer-meta > div:last-child { border-right: none; }

	.meta-label {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted);
	}

	.drawer-body { flex: 1; overflow-y: auto; padding: var(--sp-5); }

	.section-title {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: var(--sp-3);
	}
	.section-title > span:first-child {
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted);
	}

	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 2px 8px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	.account-list {
		list-style: none;
		margin: 0; padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.account-row {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: var(--sp-3);
		padding: var(--sp-3);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		align-items: center;
	}

	.acct-chain {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.82rem;
		color: var(--fg);
	}
	.acct-chain :global(svg) { border-radius: 50%; }

	.acct-addr {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.acct-addr code { font-size: 0.78rem; color: var(--fg-muted); }

	.copy-mini {
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		display: inline-flex;
		padding: 2px;
		border-radius: 3px;
	}
	.copy-mini:hover { color: var(--accent); background: var(--hover-bg); }

	.acct-balance { display: flex; flex-direction: column; align-items: flex-end; gap: 1px; }
	.tiny { font-size: 0.7rem; }

	/* ═══ Modal ═══ */
	.overlay {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(4px);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		width: 420px;
		max-width: calc(100vw - 40px);
		display: flex;
		flex-direction: column;
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
	}

	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.modal-head h3 { margin: 0; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }

	.modal-body {
		padding: var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--sp-2);
		padding: var(--sp-4) var(--sp-5);
		border-top: 1px solid var(--border);
		background: var(--surface-2);
		border-radius: 0 0 var(--r-lg) var(--r-lg);
	}

	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label {
		font-size: 0.74rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.field input[type="text"], .field input:not([type]) {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.88rem;
		padding: 9px 12px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.field input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.tier-picker {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 6px;
	}

	.tier-option {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 10px 12px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		cursor: pointer;
		text-align: left;
		font-size: 0.84rem;
		font-weight: 500;
	}
	.tier-option.active { border-color: var(--accent); background: var(--accent-soft); }
	.tier-option .tier-dot {
		width: 7px; height: 7px;
		border-radius: 50%;
		margin-bottom: 2px;
	}
	.tier-option.tone-hot .tier-dot { background: var(--red); }
	.tier-option.tone-warm .tier-dot { background: var(--yellow); }
	.tier-option.tone-cold .tier-dot { background: var(--blue); }
	.tier-hint { font-size: 0.68rem; color: var(--fg-muted); font-weight: 400; }

	.check-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		cursor: pointer;
	}
	.check-row input {
		width: 16px; height: 16px;
		accent-color: var(--accent);
		margin-top: 2px;
	}
	.check-row > div { display: flex; flex-direction: column; gap: 2px; }
	.check-label { font-size: 0.86rem; color: var(--fg); font-weight: 500; }

	/* ═══ Responsive ═══ */
	@media (max-width: 1000px) {
		.stat-strip { grid-template-columns: repeat(3, 1fr); }
	}
	@media (max-width: 700px) {
		.stat-strip { grid-template-columns: repeat(2, 1fr); }
		.drawer { width: 100vw; }
		.drawer-meta { grid-template-columns: 1fr; }
		.drawer-meta > div { border-right: none; border-bottom: 1px solid var(--border); }
		.drawer-meta > div:last-child { border-bottom: none; }
	}
</style>
