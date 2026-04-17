<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface Address {
		address: string;
		chain: string;
		user_id: string;
		whitelisted: boolean;
		created_at: string;
		label?: string;
	}

	const CHAINS = [
		{ id: 'ethereum', label: 'Ethereum' },
		{ id: 'bitcoin',  label: 'Bitcoin'  },
		{ id: 'solana',   label: 'Solana'   },
		{ id: 'tron',     label: 'Tron'     },
	];

	let addresses = $state<Address[]>([]);
	let loading = $state(true);
	let error = $state('');
	let chain = $state('ethereum');
	let copied = $state('');
	let searchQuery = $state('');
	let showWhitelistOnly = $state(false);

	async function load(c: string) {
		chain = c;
		loading = true;
		error = '';
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
		} catch { /* ignore */ }
	}

	onMount(() => load('ethereum'));

	function trunc(s: string, pre = 12, post = 8): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function fmtDate(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
	}

	let filtered = $derived(addresses.filter(a => {
		if (showWhitelistOnly && !a.whitelisted) return false;
		if (searchQuery && !a.address.toLowerCase().includes(searchQuery.toLowerCase()) && !a.user_id.toLowerCase().includes(searchQuery.toLowerCase())) return false;
		return true;
	}));

	let stats = $derived({
		total: addresses.length,
		whitelisted: addresses.filter(a => a.whitelisted).length,
	});
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>{$locale, t('addresses.title')}</h1>
		<p class="page-sub">{$locale, t('addresses.subtitle')}</p>
	</div>
</div>

<!-- ═══ Chain tabs ═══ -->
<section class="chain-tabs">
	{#each CHAINS as c}
		<button class="chain-tab" class:active={chain === c.id} onclick={() => load(c.id)}>
			<span>{@html chainIcon(c.id, 16)}</span>
			<span>{c.label}</span>
		</button>
	{/each}
</section>

<!-- ═══ Filter bar ═══ -->
<section class="filter-bar">
	<div class="search-wrap">
		<span class="search-icon">{@html icon('search', 14)}</span>
		<input type="text" bind:value={searchQuery} placeholder={($locale, t('addresses.search'))} />
	</div>
	<label class="toggle">
		<input type="checkbox" bind:checked={showWhitelistOnly} />
		<span>{$locale, t('addresses.whitelist_only')}</span>
	</label>
	<div class="stats-chip">
		<span class="muted">Total</span>
		<span class="mono strong">{stats.total}</span>
		<span class="sep">·</span>
		<span class="muted">Whitelisted</span>
		<span class="mono strong tone-ok">{stats.whitelisted}</span>
	</div>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={() => load(chain)}>Retry</button>
	</div>
{/if}

<!-- ═══ Table ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{CHAINS.find(c => c.id === chain)?.label} addresses</h2>
			<p class="panel-sub">{loading ? 'Loading…' : `${filtered.length} of ${addresses.length}`}</p>
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			{#each Array(4) as _}<div class="skel-line"></div>{/each}
		</div>
	{:else if filtered.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('addresses', 22)}</span>
			<div>
				<p class="empty-title">{addresses.length === 0 ? `No ${chain} addresses yet` : 'No matches'}</p>
				<p class="muted">{addresses.length === 0 ? 'Addresses are created on demand when users request deposits.' : 'Adjust search or toggle off "whitelisted only".'}</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>{$locale, t('addresses.col_address')}</th>
					<th>{$locale, t('addresses.col_label')}</th>
					<th>{$locale, t('addresses.col_user')}</th>
					<th>{$locale, t('addresses.col_whitelist')}</th>
					<th class="num">{$locale, t('addresses.col_created')}</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each filtered as a}
					<tr>
						<td>
							<div class="addr-cell">
								<code class="mono">{trunc(a.address)}</code>
							</div>
						</td>
						<td>
							{#if a.label}
								<span class="label-chip">{a.label}</span>
							{:else}
								<span class="muted tiny">—</span>
							{/if}
						</td>
						<td class="mono muted">{a.user_id.slice(0, 8)}</td>
						<td>
							{#if a.whitelisted}
								<span class="chip tone-ok"><span class="chip-dot"></span>{$locale, t('addresses.whitelisted')}</span>
							{:else}
								<span class="muted tiny">{$locale, t('addresses.not_whitelisted')}</span>
							{/if}
						</td>
						<td class="num mono muted">{fmtDate(a.created_at)}</td>
						<td>
							<button class="copy-btn" onclick={() => copy(a.address)}>
								{#if copied === a.address}
									<span>{@html icon('check', 12)}</span>
									<span>Copied</span>
								{:else}
									<span>{@html icon('copy', 12)}</span>
									<span>Copy</span>
								{/if}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
	}
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	/* Chain tabs */
	.chain-tabs {
		display: flex;
		gap: 4px;
		padding: 4px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		margin-bottom: var(--sp-3);
		width: fit-content;
	}
	.chain-tab {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		font-size: 0.84rem;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}
	.chain-tab:hover { background: var(--hover-bg); color: var(--fg); }
	.chain-tab.active { background: var(--accent-soft); color: var(--fg-strong); border-color: var(--accent-line); }
	.chain-tab :global(svg) { border-radius: 50%; }

	/* Filter bar */
	.filter-bar {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		margin-bottom: var(--sp-4);
		flex-wrap: wrap;
	}
	.search-wrap {
		position: relative;
		flex: 1;
		min-width: 260px;
		max-width: 400px;
	}
	.search-icon {
		position: absolute;
		left: 10px;
		top: 50%;
		transform: translateY(-50%);
		color: var(--fg-dim);
		display: inline-flex;
		pointer-events: none;
	}
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

	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 11px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		font-size: 0.82rem;
		color: var(--fg);
		cursor: pointer;
	}
	.toggle input { accent-color: var(--accent); }

	.stats-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 12px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		font-size: 0.82rem;
		margin-left: auto;
	}
	.stats-chip .sep { color: var(--fg-dim); }
	.strong { color: var(--fg-strong); font-weight: 600; }
	.strong.tone-ok { color: var(--green); }

	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }
	.alert .btn { margin-left: auto; }

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 0.8rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	/* Panel */
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
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); text-transform: capitalize; }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	/* Table */
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
	.data-table tbody td { padding: var(--sp-3) var(--sp-5); border-bottom: 1px solid var(--border); }
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover { background: var(--hover-bg); }
	.data-table th.num, .data-table td.num { text-align: right; }

	.addr-cell code { font-size: 0.82rem; color: var(--fg-strong); }

	.label-chip {
		display: inline-flex;
		font-size: 0.72rem;
		font-weight: 500;
		padding: 2px 8px;
		background: var(--accent-soft);
		color: var(--accent);
		border: 1px solid var(--accent-line);
		border-radius: var(--r-full);
	}

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
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	.copy-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 4px 9px;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		font-size: 0.75rem;
		font-weight: 500;
		cursor: pointer;
	}
	.copy-btn:hover { color: var(--accent); border-color: var(--accent-line); background: var(--accent-soft); }
	.copy-btn > span:first-child { display: inline-flex; }

	.muted { color: var(--fg-muted); }
	.tiny { font-size: 0.72rem; }

	/* Empty & skel */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
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
		height: 36px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	@media (max-width: 800px) {
		.filter-bar { flex-direction: column; align-items: stretch; }
		.stats-chip { margin-left: 0; justify-content: center; }
		.chain-tabs { width: 100%; overflow-x: auto; }
	}
</style>
