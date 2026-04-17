<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { tokenIcon, chainIcon, hasTokenIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	interface TokenInfo {
		id: string;
		symbol: string;
		name: string;
		chain: string;
		contract_address: string;
		decimals: number;
		is_native: boolean;
		enabled: boolean;
		coingecko_id?: string;
		logo_url?: string;
		is_stablecoin?: boolean;
	}

	let tokens = $state<TokenInfo[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');
	let chainFilter = $state('all');

	let showImport = $state(false);
	let importForm = $state({ symbol: '', name: '', chain: 'ethereum', contract_address: '', decimals: 6, coingecko_id: '', logo_url: '', is_stablecoin: false });
	let importing = $state(false);
	let importError = $state('');

	const CHAINS = ['all', 'ethereum', 'bitcoin', 'tron', 'solana'];

	async function loadTokens() {
		loading = true;
		error = '';
		try {
			const url = chainFilter === 'all' ? '/v1/assets' : `/v1/assets?chain=${chainFilter}`;
			tokens = await api.getJson<TokenInfo[]>(url);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load tokens';
			tokens = [];
		} finally {
			loading = false;
		}
	}

	async function importToken() {
		importing = true;
		importError = '';
		try {
			await api.post('/v1/admin/tokens/import', {
				symbol: importForm.symbol.toUpperCase(),
				name: importForm.name,
				chain: importForm.chain,
				contract_address: importForm.contract_address,
				decimals: importForm.decimals,
				coingecko_id: importForm.coingecko_id || undefined,
				logo_url: importForm.logo_url || undefined,
				is_stablecoin: importForm.is_stablecoin || undefined,
			});
			showImport = false;
			importForm = { symbol: '', name: '', chain: 'ethereum', contract_address: '', decimals: 6, coingecko_id: '', logo_url: '', is_stablecoin: false };
			await loadTokens();
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Import failed';
		} finally {
			importing = false;
		}
	}

	$effect(() => {
		chainFilter;
		loadTokens();
	});

	onMount(loadTokens);

	let filtered = $derived(tokens.filter(t => {
		if (!search) return true;
		const q = search.toLowerCase();
		return t.symbol.toLowerCase().includes(q)
			|| t.name.toLowerCase().includes(q)
			|| t.contract_address.toLowerCase().includes(q);
	}));

	let stats = $derived({
		total: tokens.length,
		native: tokens.filter(t => t.is_native).length,
		stable: tokens.filter(t => t.is_stablecoin).length,
		erc20: tokens.filter(t => !t.is_native && !t.is_stablecoin).length,
	});

	function trunc(s: string, pre = 8, post = 6): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}
</script>

<svelte:head><title>Tokens — MG Custody</title></svelte:head>

<div class="page-head">
	<div>
		<h1>Tokens</h1>
		<p class="page-sub">Supported assets across all networks. Import custom tokens by contract address.</p>
	</div>
	<button class="btn btn-primary" onclick={() => { showImport = true; importError = ''; }}>
		<span>{@html icon('plus', 14)}</span>
		<span>Import token</span>
	</button>
</div>

<section class="stat-strip">
	<div class="stat-tile"><span class="stat-label">Total</span><span class="stat-value mono">{stats.total}</span></div>
	<div class="stat-tile"><span class="stat-label">Native</span><span class="stat-value mono">{stats.native}</span></div>
	<div class="stat-tile"><span class="stat-label">Stablecoins</span><span class="stat-value mono tone-ok">{stats.stable}</span></div>
	<div class="stat-tile"><span class="stat-label">ERC-20 / TRC-20 / SPL</span><span class="stat-value mono">{stats.erc20}</span></div>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={loadTokens}>Retry</button>
	</div>
{/if}

<section class="filter-bar">
	<div class="search-wrap">
		<span class="search-icon">{@html icon('search', 14)}</span>
		<input type="text" bind:value={search} placeholder="Search by symbol, name, or contract…" />
	</div>
	<div class="chain-tabs">
		{#each CHAINS as c}
			<button class="chain-tab" class:active={chainFilter === c} onclick={() => chainFilter = c}>
				{#if c !== 'all'}{@html chainIcon(c, 14)}{/if}
				<span class="capitalize">{c === 'all' ? 'All' : c}</span>
			</button>
		{/each}
	</div>
</section>

<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Token registry</h2>
			<p class="panel-sub">{loading ? 'Loading…' : `${filtered.length} of ${tokens.length}`}</p>
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			{#each Array(4) as _}<div class="skel-line"></div>{/each}
		</div>
	{:else if filtered.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('tokens', 22)}</span>
			<div>
				<p class="empty-title">{tokens.length === 0 ? 'No tokens configured' : 'No matches'}</p>
				<p class="muted">{tokens.length === 0 ? 'Import your first token to begin supporting it for deposits and withdrawals.' : 'Try a different search or chain filter.'}</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>Asset</th>
					<th>Network</th>
					<th>Contract</th>
					<th class="num">Decimals</th>
					<th>Type</th>
					<th>Status</th>
				</tr>
			</thead>
			<tbody>
				{#each filtered as token}
					<tr>
						<td>
							<div class="asset-cell">
								{#if hasTokenIcon(token.symbol)}
									{@html tokenIcon(token.symbol, 24)}
								{:else}
									<span class="placeholder">{token.symbol.charAt(0)}</span>
								{/if}
								<div>
									<span class="sym">{token.symbol}</span>
									<span class="sub-muted">{token.name}</span>
								</div>
							</div>
						</td>
						<td>
							<div class="chain-cell">
								{@html chainIcon(token.chain, 14)}
								<span class="capitalize muted">{token.chain}</span>
							</div>
						</td>
						<td class="mono muted">{token.is_native ? '—' : trunc(token.contract_address)}</td>
						<td class="num mono">{token.decimals}</td>
						<td>
							{#if token.is_native}
								<span class="type-chip native">Native</span>
							{:else if token.is_stablecoin}
								<span class="type-chip stable">Stablecoin</span>
							{:else}
								<span class="type-chip token">Token</span>
							{/if}
						</td>
						<td>
							{#if token.enabled}
								<span class="chip tone-ok"><span class="chip-dot"></span>Active</span>
							{:else}
								<span class="chip tone-muted">Disabled</span>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

{#if showImport}
	<div class="overlay" role="presentation" onclick={() => showImport = false} onkeydown={(e) => e.key === 'Escape' && (showImport = false)}>
		<div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
			<header class="modal-head">
				<h3>Import token</h3>
				<button class="icon-btn" onclick={() => showImport = false}>{@html icon('x', 16)}</button>
			</header>

			<form class="modal-body" onsubmit={(e) => { e.preventDefault(); importToken(); }}>
				{#if importError}
					<div class="alert danger inline">
						<span>{@html icon('alert-triangle', 14)}</span>
						<span>{importError}</span>
					</div>
				{/if}

				<div class="pair">
					<div class="field">
						<label for="t-sym">Symbol</label>
						<input id="t-sym" type="text" bind:value={importForm.symbol} required placeholder="USDT" />
					</div>
					<div class="field">
						<label for="t-name">Name</label>
						<input id="t-name" type="text" bind:value={importForm.name} required placeholder="Tether USD" />
					</div>
				</div>

				<div class="pair">
					<div class="field">
						<label for="t-chain">Chain</label>
						<select id="t-chain" bind:value={importForm.chain}>
							{#each CHAINS.filter(c => c !== 'all') as c}
								<option value={c}>{c.charAt(0).toUpperCase() + c.slice(1)}</option>
							{/each}
						</select>
					</div>
					<div class="field">
						<label for="t-dec">Decimals</label>
						<input id="t-dec" type="number" bind:value={importForm.decimals} min="0" max="18" />
					</div>
				</div>

				<div class="field">
					<label for="t-addr">Contract address</label>
					<input id="t-addr" type="text" bind:value={importForm.contract_address} required placeholder="0x…" class="mono" />
				</div>

				<div class="field">
					<label for="t-cg">CoinGecko ID <span class="muted">(optional, for price feed)</span></label>
					<input id="t-cg" type="text" bind:value={importForm.coingecko_id} placeholder="tether" />
				</div>

				<label class="check-row">
					<input type="checkbox" bind:checked={importForm.is_stablecoin} />
					<span>Mark as stablecoin</span>
				</label>
			</form>

			<div class="modal-actions">
				<button class="btn btn-ghost" onclick={() => showImport = false}>Cancel</button>
				<button class="btn btn-primary" onclick={importToken} disabled={importing}>
					{#if importing}<span class="spinner"></span><span>Importing…</span>{:else}<span>Import token</span>{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
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

	.stat-strip {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 1px;
		background: var(--border);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
	}
	.stat-tile { padding: var(--sp-4) var(--sp-5); background: var(--surface); display: flex; flex-direction: column; gap: 6px; }
	.stat-label { font-size: 0.68rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.1em; color: var(--fg-muted); }
	.stat-value { font-size: 1.4rem; font-weight: 650; color: var(--fg-strong); letter-spacing: -0.02em; line-height: 1; }
	.stat-value.tone-ok { color: var(--green); }

	.alert {
		display: flex; align-items: center; gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md); font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.inline { padding: 8px 10px; font-size: 0.8rem; margin: 0; }
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }
	.alert .btn { margin-left: auto; }

	.filter-bar {
		display: flex;
		gap: var(--sp-3);
		margin-bottom: var(--sp-4);
		flex-wrap: wrap;
		align-items: center;
	}
	.search-wrap {
		position: relative;
		flex: 1;
		min-width: 240px;
		max-width: 400px;
	}
	.search-icon { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--fg-dim); display: inline-flex; pointer-events: none; }
	.search-wrap input {
		width: 100%;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.85rem;
		padding: 7px 12px 7px 32px;
		outline: none;
	}
	.search-wrap input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.chain-tabs { display: flex; gap: 4px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-md); }
	.chain-tab {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 5px 11px;
		background: transparent; border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		font-size: 0.82rem; font-weight: 500;
		cursor: pointer;
	}
	.chain-tab:hover { background: var(--hover-bg); color: var(--fg); }
	.chain-tab.active { background: var(--accent-soft); color: var(--fg-strong); border-color: var(--accent-line); }
	.chain-tab :global(svg) { border-radius: 50%; }

	.btn {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 8px 14px;
		font-size: 0.86rem; font-weight: 550;
		border-radius: var(--r-sm); border: 1px solid var(--border);
		background: var(--surface-2); color: var(--fg);
		cursor: pointer;
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
		background: transparent; border: 1px solid transparent;
		border-radius: var(--r-sm); color: var(--fg-muted); cursor: pointer;
	}
	.icon-btn:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.spinner { width: 13px; height: 13px; border: 2px solid #0A0B0D; border-top-color: transparent; border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.panel {
		background: var(--surface); border: 1px solid var(--border);
		border-radius: var(--r-lg); box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4); overflow: hidden;
	}
	.panel-head {
		display: flex; align-items: flex-start; justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	.data-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	.data-table thead th {
		text-align: left;
		padding: var(--sp-3) var(--sp-5);
		font-size: 0.68rem; font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase; letter-spacing: 0.08em;
		border-bottom: 1px solid var(--border); background: var(--surface-2);
	}
	.data-table tbody td { padding: var(--sp-3) var(--sp-5); border-bottom: 1px solid var(--border); }
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover { background: var(--hover-bg); }
	.data-table th.num, .data-table td.num { text-align: right; }

	.asset-cell { display: flex; align-items: center; gap: 10px; }
	.asset-cell :global(svg) { border-radius: 50%; flex-shrink: 0; }
	.asset-cell > div { display: flex; flex-direction: column; gap: 1px; line-height: 1.2; }
	.sym { font-weight: 600; color: var(--fg-strong); }
	.sub-muted { font-size: 0.72rem; color: var(--fg-muted); }

	.placeholder {
		display: inline-flex; align-items: center; justify-content: center;
		width: 24px; height: 24px;
		border-radius: 50%;
		background: var(--surface-2); border: 1px solid var(--border);
		font-size: 0.72rem; font-weight: 700;
		color: var(--fg-muted);
		flex-shrink: 0;
	}

	.chain-cell { display: inline-flex; align-items: center; gap: 6px; }
	.chain-cell :global(svg) { border-radius: 50%; }

	.capitalize { text-transform: capitalize; }
	.muted { color: var(--fg-muted); }

	.type-chip {
		display: inline-flex;
		font-size: 0.7rem; font-weight: 600;
		padding: 2px 8px;
		border-radius: var(--r-full);
		text-transform: capitalize;
		border: 1px solid var(--border);
	}
	.type-chip.native { background: var(--accent-soft); color: var(--accent); border-color: var(--accent-line); }
	.type-chip.stable { background: var(--green-soft); color: var(--green); border-color: rgba(52, 211, 153, 0.3); }
	.type-chip.token { background: var(--yellow-soft); color: var(--yellow); border-color: rgba(245, 185, 66, 0.3); }

	.chip {
		display: inline-flex; align-items: center; gap: 5px;
		font-size: 0.72rem; font-weight: 500;
		padding: 2px 9px; border-radius: var(--r-full); border: 1px solid var(--border);
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-muted { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	.panel-empty {
		display: flex; align-items: center; gap: var(--sp-4);
		padding: var(--sp-6);
	}
	.empty-mark {
		display: inline-flex; align-items: center; justify-content: center;
		width: 44px; height: 44px;
		background: var(--accent-soft); color: var(--accent);
		border-radius: var(--r-md); flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-line {
		height: 44px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	.overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
	.modal { background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); width: 480px; max-width: calc(100vw - 40px); display: flex; flex-direction: column; box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5); }
	.modal-head { display: flex; align-items: center; justify-content: space-between; padding: var(--sp-4) var(--sp-5); border-bottom: 1px solid var(--border); }
	.modal-head h3 { margin: 0; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }
	.modal-body { padding: var(--sp-5); display: flex; flex-direction: column; gap: var(--sp-4); }
	.modal-actions { display: flex; justify-content: flex-end; gap: var(--sp-2); padding: var(--sp-4) var(--sp-5); border-top: 1px solid var(--border); background: var(--surface-2); border-radius: 0 0 var(--r-lg) var(--r-lg); }

	.pair { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-3); }

	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label { font-size: 0.72rem; font-weight: 600; color: var(--fg-muted); text-transform: uppercase; letter-spacing: 0.08em; }
	.field input, .field select {
		background: var(--surface-2); border: 1px solid var(--border);
		border-radius: var(--r-md); color: var(--fg);
		font-size: 0.88rem; padding: 9px 12px; outline: none;
	}
	.field input.mono { font-family: var(--font-mono); }
	.field input:focus, .field select:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.check-row { display: flex; align-items: center; gap: 10px; cursor: pointer; }
	.check-row input { width: 16px; height: 16px; accent-color: var(--accent); }

	@media (max-width: 900px) {
		.stat-strip { grid-template-columns: repeat(2, 1fr); }
	}
</style>
