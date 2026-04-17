<script lang="ts">
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface DerivedAddress {
		index: number;
		path: string;
		address: string;
		used: boolean;
	}

	interface DerivationTree {
		chain: string;
		base_path: string;
		depth: number;
		addresses: DerivedAddress[];
	}

	const CHAINS = [
		{ id: 'bitcoin',  label: 'Bitcoin'  },
		{ id: 'ethereum', label: 'Ethereum' },
		{ id: 'solana',   label: 'Solana'   },
		{ id: 'tron',     label: 'Tron'     },
	];

	let chain = $state('ethereum');
	let depth = $state(10);
	let tree = $state<DerivationTree | null>(null);
	let loading = $state(false);
	let error = $state('');
	let verifyResult = $state<{ ok: boolean; message: string } | null>(null);
	let verifyingIdx = $state<number | null>(null);
	let copied = $state('');

	async function loadTree() {
		loading = true;
		error = '';
		tree = null;
		try {
			tree = await api.getJson<DerivationTree>(`/v1/admin/derivation-tree/${chain}?depth=${depth}`);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function verifyAddress(idx: number, expected: string) {
		verifyingIdx = idx;
		verifyResult = null;
		try {
			const res = await api.getJson<DerivationTree>(`/v1/admin/derivation-tree/${chain}?depth=${idx + 1}`);
			const addr = res.addresses.find(a => a.index === idx);
			if (addr && addr.address === expected) {
				verifyResult = { ok: true, message: `Index ${idx}: address matches deterministic derivation` };
			} else {
				verifyResult = { ok: false, message: `Index ${idx}: MISMATCH — expected ${expected}, got ${addr?.address ?? 'not found'}` };
			}
		} catch (e) {
			verifyResult = { ok: false, message: `Verify failed: ${String(e)}` };
		} finally {
			verifyingIdx = null;
		}
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			copied = text;
			setTimeout(() => { if (copied === text) copied = ''; }, 1600);
		} catch { /* ignore */ }
	}

	function trunc(s: string, pre = 10, post = 8): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	let stats = $derived(tree ? {
		used: tree.addresses.filter(a => a.used).length,
		unused: tree.addresses.filter(a => !a.used).length,
		total: tree.addresses.length,
	} : null);
</script>

<div class="page-head">
	<div>
		<h1>{$locale, t('derivation.title')}</h1>
		<p class="page-sub">{$locale, t('derivation.subtitle')}</p>
	</div>
</div>

<section class="controls">
	<div class="field">
		<label for="d-chain">{$locale, t('derivation.chain')}</label>
		<div class="select-wrap">
			<span class="select-icon">{@html chainIcon(chain, 16)}</span>
			<select id="d-chain" bind:value={chain}>
				{#each CHAINS as c}<option value={c.id}>{c.label}</option>{/each}
			</select>
			<span class="select-chev">{@html icon('chevron-down', 14)}</span>
		</div>
	</div>

	<div class="field">
		<label for="d-depth">Depth <span class="muted tiny">(1–100)</span></label>
		<input id="d-depth" type="number" bind:value={depth} min="1" max="100" />
	</div>

	<button class="btn btn-primary" onclick={loadTree} disabled={loading}>
		{#if loading}
			<span class="spinner"></span>
			<span>{$locale, t('derivation.deriving')}</span>
		{:else}
			<span>{@html icon('derivation', 14)}</span>
			<span>{$locale, t('derivation.derive')}</span>
		{/if}
	</button>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
	</div>
{/if}

{#if verifyResult}
	<div class="alert {verifyResult.ok ? 'success' : 'danger'}">
		<span>{@html icon(verifyResult.ok ? 'shield-check' : 'alert-triangle', 16)}</span>
		<span>{verifyResult.message}</span>
		<button class="btn btn-ghost sm" onclick={() => verifyResult = null}>Dismiss</button>
	</div>
{/if}

{#if tree && stats}
	<section class="stat-strip">
		<div class="stat-tile">
			<span class="stat-label">{$locale, t('derivation.stat_base')}</span>
			<span class="stat-value mono path-val">{tree.base_path}</span>
		</div>
		<div class="stat-tile">
			<span class="stat-label">{$locale, t('derivation.stat_derived')}</span>
			<span class="stat-value mono">{stats.total}</span>
		</div>
		<div class="stat-tile">
			<span class="stat-label">{$locale, t('derivation.stat_used')}</span>
			<span class="stat-value mono tone-ok">{stats.used}</span>
		</div>
		<div class="stat-tile">
			<span class="stat-label">{$locale, t('derivation.stat_unused')}</span>
			<span class="stat-value mono muted">{stats.unused}</span>
		</div>
	</section>

	<section class="panel">
		<header class="panel-head">
			<div>
				<h2 class="capitalize">
					<span class="chain-ico">{@html chainIcon(tree.chain, 16)}</span>
					{tree.chain} addresses
				</h2>
				<p class="panel-sub">{$locale, t('derivation.addr_table_sub')}</p>
			</div>
		</header>

		<table class="data-table">
			<thead>
				<tr>
					<th class="num">#</th>
					<th>{$locale, t('derivation.col_path')}</th>
					<th>{$locale, t('derivation.col_address')}</th>
					<th>{$locale, t('derivation.col_status')}</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each tree.addresses as a}
					<tr>
						<td class="num mono muted">{a.index}</td>
						<td><code class="mono path-code">{a.path}</code></td>
						<td>
							<div class="addr-cell">
								<code class="mono">{trunc(a.address, 14, 10)}</code>
								<button class="icon-btn sm" onclick={() => copy(a.address)} title="Copy address">
									{#if copied === a.address}
										{@html icon('check', 12)}
									{:else}
										{@html icon('copy', 12)}
									{/if}
								</button>
							</div>
						</td>
						<td>
							{#if a.used}
								<span class="chip tone-ok"><span class="chip-dot"></span>{$locale, t('derivation.status_used')}</span>
							{:else}
								<span class="chip tone-muted">{$locale, t('derivation.status_unused')}</span>
							{/if}
						</td>
						<td>
							<button class="btn btn-ghost sm" onclick={() => verifyAddress(a.index, a.address)} disabled={verifyingIdx === a.index}>
								{#if verifyingIdx === a.index}
									<span class="spinner sm-spin"></span>
								{:else}
									<span>{@html icon('shield-check', 12)}</span>
									<span>{$locale, t('derivation.verify')}</span>
								{/if}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</section>
{/if}

<style>
	.page-head { margin-bottom: var(--sp-5); }
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	.controls {
		display: flex;
		align-items: flex-end;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
		flex-wrap: wrap;
	}

	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label {
		font-size: 0.72rem; font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase; letter-spacing: 0.08em;
	}

	.select-wrap { position: relative; display: flex; align-items: center; min-width: 180px; }
	.select-icon { position: absolute; left: 10px; display: inline-flex; pointer-events: none; z-index: 1; }
	.select-icon :global(svg) { border-radius: 50%; }
	.select-chev { position: absolute; right: 10px; color: var(--fg-dim); display: inline-flex; pointer-events: none; }
	.select-wrap select {
		appearance: none; -webkit-appearance: none;
		width: 100%;
		background: var(--surface-2); border: 1px solid var(--border);
		border-radius: var(--r-md); color: var(--fg);
		font-size: 0.88rem; padding: 9px 32px 9px 36px;
		outline: none; cursor: pointer;
	}
	.select-wrap select:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.field input {
		width: 140px;
		background: var(--surface-2); border: 1px solid var(--border);
		border-radius: var(--r-md); color: var(--fg);
		font-size: 0.88rem; padding: 9px 12px;
		outline: none;
	}
	.field input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.btn {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 9px 14px;
		font-size: 0.86rem; font-weight: 550;
		border-radius: var(--r-sm); border: 1px solid var(--border);
		background: var(--surface-2); color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.74rem; }
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: #0A0B0D; border-color: var(--accent); font-weight: 600; }
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.spinner { width: 13px; height: 13px; border: 2px solid #0A0B0D; border-top-color: transparent; border-radius: 50%; animation: spin 0.7s linear infinite; }
	.sm-spin { width: 11px; height: 11px; border: 2px solid var(--fg-muted); border-top-color: transparent; border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.alert {
		display: flex; align-items: center; gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md); font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.success { background: var(--green-soft); color: var(--green); border: 1px solid rgba(52, 211, 153, 0.25); }
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }
	.alert .btn { margin-left: auto; }

	.stat-strip {
		display: grid;
		grid-template-columns: 2fr 1fr 1fr 1fr;
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
	.stat-value { font-size: 1.3rem; font-weight: 650; color: var(--fg-strong); letter-spacing: -0.02em; line-height: 1; }
	.stat-value.tone-ok { color: var(--green); }
	.stat-value.muted { color: var(--fg-muted); }
	.path-val { font-size: 0.95rem; font-weight: 500; color: var(--yellow); }

	.panel {
		background: var(--surface); border: 1px solid var(--border);
		border-radius: var(--r-lg); box-shadow: var(--shadow-1);
		overflow: hidden; margin-bottom: var(--sp-4);
	}
	.panel-head {
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 {
		margin: 0 0 2px;
		font-size: 0.92rem; font-weight: 600;
		color: var(--fg-strong);
		display: flex; align-items: center; gap: 8px;
	}
	.chain-ico { display: inline-flex; }
	.chain-ico :global(svg) { border-radius: 50%; }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }

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

	.path-code {
		display: inline-block;
		padding: 2px 7px;
		background: var(--yellow-soft);
		color: var(--yellow);
		border-radius: var(--r-sm);
		font-size: 0.78rem;
		font-weight: 500;
	}

	.addr-cell {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.addr-cell code { font-size: 0.82rem; color: var(--fg-strong); }

	.icon-btn {
		display: inline-flex; align-items: center; justify-content: center;
		width: 22px; height: 22px;
		background: transparent; border: 1px solid transparent;
		border-radius: 4px;
		color: var(--fg-dim); cursor: pointer;
	}
	.icon-btn.sm { width: 20px; height: 20px; }
	.icon-btn:hover { color: var(--accent); background: var(--accent-soft); }

	.chip {
		display: inline-flex; align-items: center; gap: 5px;
		font-size: 0.72rem; font-weight: 500;
		padding: 2px 9px;
		border-radius: var(--r-full); border: 1px solid var(--border);
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-muted { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	.capitalize { text-transform: capitalize; }
	.muted { color: var(--fg-muted); }
	.tiny { font-size: 0.7rem; font-weight: 400; letter-spacing: 0; text-transform: none; }
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	@media (max-width: 900px) {
		.stat-strip { grid-template-columns: 1fr 1fr; }
	}
</style>
