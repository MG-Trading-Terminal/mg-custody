<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { tokenIcon, chainIcon, hasTokenIcon } from '$lib/icons/tokens';

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

	// Import modal
	let showImport = $state(false);
	let importForm = $state({ symbol: '', name: '', chain: 'ethereum', contract_address: '', decimals: 6, coingecko_id: '', logo_url: '', is_stablecoin: false });
	let importing = $state(false);
	let importError = $state('');

	const CHAINS = ['all', 'ethereum', 'bitcoin', 'tron', 'solana', 'arbitrum', 'optimism', 'base', 'polygon', 'avalanche', 'bsc'];

	async function loadTokens() {
		loading = true;
		error = '';
		try {
			const url = chainFilter === 'all' ? '/v1/assets' : `/v1/assets?chain=${chainFilter}`;
			tokens = await api.getJson<TokenInfo[]>(url);
		} catch (e) {
			// Fallback: load from token database
			try {
				const resp = await api.getJson<{ tokens: TokenInfo[] }>('/v1/sdk/endpoints');
				tokens = [];
			} catch {
				error = e instanceof Error ? e.message : 'Failed to load tokens';
				tokens = [];
			}
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
		// Reload when chain filter changes
		chainFilter;
		loadTokens();
	});

	onMount(loadTokens);

	function filtered(): TokenInfo[] {
		if (!search) return tokens;
		const q = search.toLowerCase();
		return tokens.filter(t =>
			t.symbol.toLowerCase().includes(q) ||
			t.name.toLowerCase().includes(q) ||
			t.contract_address.toLowerCase().includes(q)
		);
	}

	function trunc(s: string, n = 16): string {
		return s.length > n ? s.slice(0, 8) + '...' + s.slice(-6) : s;
	}
</script>

<svelte:head><title>Tokens — MG Custody</title></svelte:head>

<h1>Supported Tokens</h1>

<div class="toolbar">
	<div class="filters">
		<input type="text" placeholder="Search symbol, name, or address..." bind:value={search} class="search" />
		<select bind:value={chainFilter} class="chain-select">
			{#each CHAINS as c}
				<option value={c}>
					{c === 'all' ? 'All Chains' : c.charAt(0).toUpperCase() + c.slice(1)}
				</option>
			{/each}
		</select>
	</div>
	<button class="btn-primary" onclick={() => { showImport = true; }}>+ Import Token</button>
</div>

{#if error}
	<div class="card err">{error}</div>
{/if}

{#if loading}
	<div class="card muted">Loading tokens...</div>
{:else}
	<div class="card">
		<div class="token-count">{filtered().length} token{filtered().length !== 1 ? 's' : ''}</div>
		<div class="table-wrap"><table>
			<thead>
				<tr>
					<th>Token</th>
					<th>Chain</th>
					<th>Contract</th>
					<th>Decimals</th>
					<th>Type</th>
					<th>Status</th>
				</tr>
			</thead>
			<tbody>
				{#each filtered() as token}
					<tr>
						<td class="token-cell-td">
							<span class="token-cell">
								{#if hasTokenIcon(token.symbol)}
									{@html tokenIcon(token.symbol, 22)}
								{:else}
									<span class="icon-placeholder">{token.symbol.charAt(0)}</span>
								{/if}
								<span>
									<strong>{token.symbol}</strong>
									<span class="token-name">{token.name}</span>
								</span>
							</span>
						</td>
						<td>
							<span class="token-cell">
								{@html chainIcon(token.chain, 16)}
								{token.chain}
							</span>
						</td>
						<td class="mono">{token.is_native ? 'Native' : trunc(token.contract_address)}</td>
						<td>{token.decimals}</td>
						<td>
							{#if token.is_native}
								<span class="badge native">Native</span>
							{:else if token.is_stablecoin}
								<span class="badge stable">Stablecoin</span>
							{:else}
								<span class="badge erc20">Token</span>
							{/if}
						</td>
						<td>
							<span class="status" class:active={token.enabled}>{token.enabled ? 'Active' : 'Disabled'}</span>
						</td>
					</tr>
				{/each}
				{#if filtered().length === 0}
					<tr><td colspan="6" class="muted" style="text-align:center;padding:2rem">
						{search ? 'No tokens match your search.' : 'No tokens configured. Import your first token.'}
					</td></tr>
				{/if}
			</tbody>
		</table></div>
	</div>
{/if}

<!-- Import Modal -->
{#if showImport}
	<div class="modal-backdrop" role="presentation" onclick={() => { showImport = false; }}></div>
	<div class="modal">
		<h2>Import Token</h2>
		<form onsubmit={(e) => { e.preventDefault(); importToken(); }}>
			{#if importError}
				<p class="err-text">{importError}</p>
			{/if}
			<div class="form-row">
				<label>
					Symbol
					<input type="text" bind:value={importForm.symbol} required placeholder="USDT" />
				</label>
				<label>
					Name
					<input type="text" bind:value={importForm.name} required placeholder="Tether USD" />
				</label>
			</div>
			<div class="form-row">
				<label>
					Chain
					<select bind:value={importForm.chain}>
						{#each CHAINS.filter(c => c !== 'all') as c}
							<option value={c}>{c}</option>
						{/each}
					</select>
				</label>
				<label>
					Decimals
					<input type="number" bind:value={importForm.decimals} min="0" max="18" />
				</label>
			</div>
			<label>
				Contract Address
				<input type="text" bind:value={importForm.contract_address} required placeholder="0x..." />
			</label>
			<label>
				CoinGecko ID (optional)
				<input type="text" bind:value={importForm.coingecko_id} placeholder="tether" />
			</label>
			<label class="checkbox-label">
				<input type="checkbox" bind:checked={importForm.is_stablecoin} />
				Stablecoin
			</label>
			<div class="modal-actions">
				<button type="button" class="btn-secondary" onclick={() => { showImport = false; }}>Cancel</button>
				<button type="submit" class="btn-primary" disabled={importing}>{importing ? 'Importing...' : 'Import'}</button>
			</div>
		</form>
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.toolbar { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; gap: 1rem; flex-wrap: wrap; }
	.filters { display: flex; gap: 0.5rem; flex: 1; }
	.search { flex: 1; min-width: 200px; background: var(--hover-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--fg); padding: 0.5rem 0.75rem; font-size: 0.9rem; }
	.chain-select { background: var(--hover-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--fg); padding: 0.5rem; font-size: 0.85rem; }

	.card { background: var(--card-bg); border: 1px solid var(--border); border-radius: 8px; padding: 1.2rem; margin-bottom: 1rem; }
	.err { color: var(--red); border-color: rgba(248, 81, 73, 0.3); }

	.token-count { font-size: 0.82rem; color: var(--fg-muted); margin-bottom: 0.75rem; }

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted); border-bottom: 1px solid var(--border); font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border); vertical-align: middle; }

	.token-cell-td { min-width: 160px; }

	.token-cell strong { display: block; font-size: 0.9rem; }
	.token-name { display: block; font-size: 0.75rem; color: var(--fg-muted); }

	.icon-placeholder { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; border-radius: 50%; background: var(--hover-bg); border: 1px solid var(--border); font-size: 0.7rem; font-weight: 700; color: var(--fg-muted); flex-shrink: 0; }

	.mono { font-family: monospace; font-size: 0.8rem; color: var(--fg-muted); }

	.badge { display: inline-block; padding: 0.1rem 0.4rem; border-radius: 4px; font-size: 0.72rem; font-weight: 600; text-transform: uppercase; }
	.badge.native { background: rgba(88, 166, 255, 0.15); color: var(--accent); }
	.badge.stable { background: rgba(63, 185, 80, 0.15); color: var(--green); }
	.badge.erc20 { background: rgba(210, 153, 34, 0.15); color: var(--yellow); }

	.status { font-size: 0.82rem; }
	.status.active { color: var(--green); }

	.btn-primary { background: var(--accent); color: #0f1117; border: none; border-radius: 6px; padding: 0.5rem 1rem; font-size: 0.85rem; font-weight: 600; cursor: pointer; }
	.btn-primary:disabled { opacity: 0.6; }
	.btn-secondary { background: var(--hover-bg); color: var(--fg); border: 1px solid var(--border); border-radius: 6px; padding: 0.5rem 1rem; font-size: 0.85rem; cursor: pointer; }

	.modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.6); z-index: 200; }
	.modal { position: fixed; top: 50%; left: 50%; transform: translate(-50%,-50%); background: var(--card-bg); border: 1px solid var(--border); border-radius: 8px; padding: 1.5rem; z-index: 201; width: 90%; max-width: 480px; }
	.modal h2 { margin: 0 0 1rem; font-size: 1.1rem; }
	.modal label { display: flex; flex-direction: column; gap: 0.3rem; font-size: 0.85rem; color: var(--fg-muted); margin-bottom: 0.6rem; }
	.modal input, .modal select { background: var(--hover-bg); border: 1px solid var(--border); border-radius: 6px; color: var(--fg); padding: 0.5rem 0.75rem; font-size: 0.9rem; }
	.form-row { display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem; }
	.checkbox-label { flex-direction: row !important; align-items: center; gap: 0.5rem; }
	.checkbox-label input { width: auto; }
	.modal-actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1rem; }
	.err-text { color: var(--red); font-size: 0.85rem; margin: 0 0 0.5rem; }
	.muted { color: var(--fg-muted); }
</style>
