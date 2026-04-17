<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon, tokenIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	interface FeeEstimate {
		amount: string;
		network_fee: string;
		total_debit?: string;
		total?: string;
		asset?: string;
		estimated_time_seconds?: number;
	}

	interface WithdrawalRecord {
		id: string;
		asset: string;
		chain: string;
		amount: string;
		destination: string;
		status: string;
		tx_hash?: string;
		created_at: string;
	}

	const CHAINS = [
		{ id: 'ethereum', label: 'Ethereum' },
		{ id: 'bitcoin',  label: 'Bitcoin'  },
		{ id: 'solana',   label: 'Solana'   },
		{ id: 'tron',     label: 'Tron'     },
	];

	const ASSETS_BY_CHAIN: Record<string, string[]> = {
		ethereum: ['ETH','USDT','USDC','DAI'],
		bitcoin:  ['BTC'],
		solana:   ['SOL','USDT','USDC'],
		tron:     ['TRX','USDT'],
	};

	// Withdrawal tier thresholds (USD) — aligns with CLAUDE.md circuit breaker config
	const TIERS = [
		{ id: 0, max: 1_000,    label: 'Tier 0 · Auto', approval: 'Auto-approved (whitelisted)', delay: '1–5m',  tone: 'ok'   },
		{ id: 1, max: 10_000,   label: 'Tier 1 · Auto', approval: 'Auto + alert',                delay: '10m',   tone: 'ok'   },
		{ id: 2, max: 100_000,  label: 'Tier 2',        approval: '1 approver (FIDO2)',          delay: '30m',   tone: 'warn' },
		{ id: 3, max: 500_000,  label: 'Tier 3',        approval: '2 approvers',                  delay: '2h',    tone: 'warn' },
		{ id: 4, max: Infinity, label: 'Tier 4',        approval: '3 approvers · cold wallet',    delay: '24h',   tone: 'crit' },
	];

	// Stub asset prices (frontend has no price oracle yet — real system pulls from /v1/market/prices)
	const ASSET_USD: Record<string, number> = {
		BTC: 68000, ETH: 3500, SOL: 180, TRX: 0.18,
		USDT: 1, USDC: 1, DAI: 1, MATIC: 0.85,
	};

	let chain       = $state('ethereum');
	let asset       = $state('ETH');
	let amount      = $state('');
	let destination = $state('');

	let estimate    = $state<FeeEstimate | null>(null);
	let estLoading  = $state(false);
	let estError    = $state('');

	let submitting  = $state(false);
	let submitError = $state('');
	let submitOk    = $state('');

	let history     = $state<WithdrawalRecord[]>([]);
	let histLoading = $state(true);
	let histError   = $state('');

	$effect(() => {
		const avail = ASSETS_BY_CHAIN[chain] ?? [];
		if (!avail.includes(asset)) asset = avail[0] ?? '';
		estimate = null;
		estError = '';
	});

	let amountUsd = $derived(() => {
		const a = parseFloat(amount);
		const p = ASSET_USD[asset] ?? 0;
		return isFinite(a) && isFinite(p) ? a * p : 0;
	});

	let currentTier = $derived(TIERS.find(t => (amountUsd() ?? 0) <= t.max) ?? TIERS[TIERS.length - 1]);

	async function estimateFee() {
		if (!amount || !destination) { estError = 'Enter amount and destination first'; return; }
		estLoading = true;
		estError = '';
		estimate = null;
		try {
			estimate = await api.post<FeeEstimate>('/v1/user/withdrawals/estimate', {
				chain, asset, amount, destination,
			});
		} catch (e) {
			estError = e instanceof Error ? e.message : 'Fee estimation failed';
		} finally {
			estLoading = false;
		}
	}

	async function submit() {
		submitError = '';
		submitOk = '';
		if (!amount || !destination) { submitError = 'All fields are required'; return; }
		submitting = true;
		try {
			const res = await api.post<{ custody_ref: string }>('/v1/withdrawals', {
				chain, asset, amount, address: destination,
			});
			submitOk = res.custody_ref;
			amount = '';
			destination = '';
			estimate = null;
			fetchHistory();
		} catch (e) {
			submitError = e instanceof Error ? e.message : 'Submission failed';
		} finally {
			submitting = false;
		}
	}

	async function fetchHistory() {
		histLoading = true;
		histError = '';
		try {
			const data = await api.getJson<{ withdrawals: WithdrawalRecord[] }>('/v1/user/withdrawal-history');
			history = data.withdrawals ?? [];
		} catch (e) {
			histError = e instanceof Error ? e.message : 'Failed to load withdrawal history';
		} finally {
			histLoading = false;
		}
	}

	onMount(fetchHistory);

	function statusTone(s: string): 'success' | 'pending' | 'danger' | 'muted' {
		if (s === 'confirmed') return 'success';
		if (s === 'broadcast' || s === 'pending' || s === 'signed' || s === 'validated' || s === 'received') return 'pending';
		if (s === 'failed' || s === 'rejected') return 'danger';
		return 'muted';
	}

	function fmtUsd(n: number): string {
		if (!isFinite(n) || n <= 0) return '$0.00';
		return n.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
	}

	function trunc(s: string, pre = 10, post = 8): string {
		if (!s) return '—';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function fmtDate(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
		});
	}

	function isAddrPlausible(a: string): boolean {
		if (!a) return false;
		// Basic length/charset check — real validation happens server-side
		if (chain === 'ethereum' || chain === 'arbitrum' || chain === 'optimism' || chain === 'base' || chain === 'polygon') {
			return /^0x[a-fA-F0-9]{40}$/.test(a);
		}
		if (chain === 'bitcoin') {
			return /^(bc1|[13])[a-zA-HJ-NP-Z0-9]{25,62}$/.test(a);
		}
		if (chain === 'tron') {
			return /^T[1-9A-HJ-NP-Za-km-z]{33}$/.test(a);
		}
		if (chain === 'solana') {
			return /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(a);
		}
		return a.length >= 20;
	}

	function totalDebit(est: FeeEstimate): string {
		return est.total_debit ?? est.total ?? '—';
	}
</script>

<!-- ═════════════════ Header ═════════════════ -->
<div class="page-head">
	<div>
		<h1>Withdraw</h1>
		<p class="page-sub">Initiate an on-chain transfer. All withdrawals are subject to policy engine rules.</p>
	</div>
</div>

<!-- ═════════════════ Main grid ═════════════════ -->
<div class="withdraw-grid">
	<!-- Form -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>New withdrawal</h2>
				<p class="panel-sub">Double-check destination — blockchain transfers are irreversible.</p>
			</div>
		</header>

		<div class="form-body">
			<!-- Chain + asset row -->
			<div class="pair-row">
				<div class="field">
					<label for="chain">Network</label>
					<div class="select-wrap">
						<span class="select-icon">{@html chainIcon(chain, 16)}</span>
						<select id="chain" bind:value={chain}>
							{#each CHAINS as c}
								<option value={c.id}>{c.label}</option>
							{/each}
						</select>
						<span class="select-chev">{@html icon('chevron-down', 14)}</span>
					</div>
				</div>

				<div class="field">
					<label for="asset">Asset</label>
					<div class="select-wrap">
						<span class="select-icon">{@html tokenIcon(asset, 16)}</span>
						<select id="asset" bind:value={asset}>
							{#each (ASSETS_BY_CHAIN[chain] ?? []) as a}
								<option value={a}>{a}</option>
							{/each}
						</select>
						<span class="select-chev">{@html icon('chevron-down', 14)}</span>
					</div>
				</div>
			</div>

			<!-- Amount -->
			<div class="field">
				<div class="label-row">
					<label for="amount">Amount</label>
					<span class="label-aux mono">≈ {fmtUsd(amountUsd() ?? 0)}</span>
				</div>
				<div class="amount-wrap">
					<input
						id="amount"
						type="text"
						inputmode="decimal"
						bind:value={amount}
						placeholder="0.00"
						class="amount-input mono"
					/>
					<span class="amount-suffix">{asset}</span>
				</div>
			</div>

			<!-- Destination -->
			<div class="field">
				<div class="label-row">
					<label for="dest">Destination address</label>
					{#if destination}
						<span class="label-aux {isAddrPlausible(destination) ? 'ok' : 'warn'}">
							{isAddrPlausible(destination) ? '✓ Format looks valid' : '⚠ Format mismatch'}
						</span>
					{/if}
				</div>
				<input
					id="dest"
					type="text"
					bind:value={destination}
					placeholder={chain === 'ethereum' ? '0x…' : chain === 'bitcoin' ? 'bc1…' : chain === 'tron' ? 'T…' : 'Paste address'}
					class="addr-input mono"
					spellcheck="false"
				/>
			</div>

			<!-- Tier indicator -->
			<div class="tier-panel">
				<div class="tier-head">
					<span class="tier-label">Approval tier</span>
					<span class="tier-badge tone-{currentTier.tone}">{currentTier.label}</span>
				</div>
				<div class="tier-body">
					<div>
						<span class="tier-meta-label">Approval</span>
						<span class="tier-meta-val">{currentTier.approval}</span>
					</div>
					<div>
						<span class="tier-meta-label">Time delay</span>
						<span class="tier-meta-val mono">{currentTier.delay}</span>
					</div>
				</div>
				<!-- Tier progress -->
				<div class="tier-bar">
					{#each TIERS.filter(t => t.id < 4) as t}
						<div class="tier-seg tone-{t.tone}" class:active={t.id <= currentTier.id}></div>
					{/each}
				</div>
			</div>

			<!-- Fee estimate -->
			{#if estimate}
				<div class="fee-panel">
					<div class="fee-row">
						<span class="fee-label">Amount</span>
						<span class="mono">{estimate.amount} {asset}</span>
					</div>
					<div class="fee-row">
						<span class="fee-label">Network fee</span>
						<span class="mono fee-highlight">{estimate.network_fee} {asset}</span>
					</div>
					{#if estimate.estimated_time_seconds}
						<div class="fee-row">
							<span class="fee-label">Estimated time</span>
							<span class="mono">~{Math.round((estimate.estimated_time_seconds ?? 0) / 60)}m</span>
						</div>
					{/if}
					<div class="fee-row total">
						<span class="fee-label">Total debit</span>
						<span class="mono strong">{totalDebit(estimate)} {asset}</span>
					</div>
				</div>
			{:else if estError}
				<div class="alert danger inline">
					<span>{@html icon('alert-triangle', 14)}</span>
					<span>{estError}</span>
				</div>
			{/if}

			<!-- Submit result -->
			{#if submitOk}
				<div class="alert success">
					<span>{@html icon('check', 16)}</span>
					<div>
						<span class="alert-title">Withdrawal submitted</span>
						<span class="alert-sub mono">Ref {submitOk}</span>
					</div>
				</div>
			{/if}
			{#if submitError}
				<div class="alert danger">
					<span>{@html icon('alert-triangle', 16)}</span>
					<span>{submitError}</span>
				</div>
			{/if}

			<!-- Actions -->
			<div class="form-actions">
				<button class="btn btn-secondary" onclick={estimateFee} disabled={estLoading || !amount || !destination}>
					{#if estLoading}
						<span class="spinner sm"></span>
						<span>Estimating…</span>
					{:else}
						<span>{@html icon('sliders', 14)}</span>
						<span>Estimate fee</span>
					{/if}
				</button>
				<button
					class="btn btn-primary"
					onclick={submit}
					disabled={submitting || !amount || !destination || !isAddrPlausible(destination)}
				>
					{#if submitting}
						<span class="spinner"></span>
						<span>Submitting…</span>
					{:else}
						<span>Submit withdrawal</span>
						<span>{@html icon('arrow-up-right', 14)}</span>
					{/if}
				</button>
			</div>
		</div>
	</section>

	<!-- Info sidebar -->
	<aside class="panel info-panel">
		<header class="panel-head">
			<h2>
				<span class="info-icon">{@html icon('shield-check', 14)}</span>
				What happens next
			</h2>
		</header>

		<ol class="flow-steps">
			<li class="active">
				<span class="step-dot"></span>
				<div>
					<strong>Policy check</strong>
					<p>Rules engine validates destination, amount, and asset against whitelist.</p>
				</div>
			</li>
			<li>
				<span class="step-dot"></span>
				<div>
					<strong>Approval ({currentTier.approval})</strong>
					<p>Required signatures collected. Time delay: {currentTier.delay}.</p>
				</div>
			</li>
			<li>
				<span class="step-dot"></span>
				<div>
					<strong>Sign &amp; broadcast</strong>
					<p>Signing service validates HMAC, signs, submits to network.</p>
				</div>
			</li>
			<li>
				<span class="step-dot"></span>
				<div>
					<strong>Confirmation</strong>
					<p>Credited after required confirmations. Webhook fires to exchange.</p>
				</div>
			</li>
		</ol>

		<div class="info-footer">
			<div class="info-meta">
				<span>{@html icon('lock', 12)}</span>
				<span>Destination address is compared against approval-bound record before signing</span>
			</div>
			<div class="info-meta">
				<span>{@html icon('activity', 12)}</span>
				<span>Circuit breaker: $500K per tx · $10M/hr · $50M/day</span>
			</div>
		</div>
	</aside>
</div>

<!-- ═════════════════ History ═════════════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Recent withdrawals</h2>
			<p class="panel-sub">Outgoing transfers — latest {Math.min(history.length, 50)} of {history.length}</p>
		</div>
		<span class="badge-count">{history.length}</span>
	</header>

	{#if histLoading}
		<div class="panel-body">
			<div class="skel-line"></div>
			<div class="skel-line"></div>
		</div>
	{:else if histError}
		<div class="alert danger" style="margin: var(--sp-5)">
			<span>{@html icon('alert-triangle', 16)}</span>
			<span>{histError}</span>
		</div>
	{:else if history.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('arrow-up-right', 22)}</span>
			<div>
				<p class="empty-title">No withdrawals yet</p>
				<p class="muted">Outgoing transfers appear here once submitted. All are policy-checked.</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>Asset</th>
					<th>Network</th>
					<th class="num">Amount</th>
					<th>Destination</th>
					<th>Tx hash</th>
					<th>Status</th>
					<th class="num">When</th>
				</tr>
			</thead>
			<tbody>
				{#each history.slice(0, 50) as w}
					<tr>
						<td>
							<div class="token-cell">
								{@html tokenIcon(w.asset, 18)}
								<span class="strong">{w.asset}</span>
							</div>
						</td>
						<td class="capitalize muted">{w.chain}</td>
						<td class="num mono strong">{w.amount}</td>
						<td class="mono muted">{trunc(w.destination)}</td>
						<td class="mono muted">{w.tx_hash ? trunc(w.tx_hash) : '—'}</td>
						<td><span class="status-pill {statusTone(w.status)}">{w.status}</span></td>
						<td class="num mono muted">{fmtDate(w.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
	/* ─── Page head ─── */
	.page-head { margin-bottom: var(--sp-5); }
	h1 {
		margin: 0 0 4px;
		font-size: 1.5rem;
		font-weight: 650;
		letter-spacing: -0.022em;
	}
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	/* ─── Panel ─── */
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

	.panel-head h2 {
		margin: 0 0 2px;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }

	.info-icon { display: inline-flex; color: var(--accent); }

	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	/* ─── Layout ─── */
	.withdraw-grid {
		display: grid;
		grid-template-columns: 1.3fr 1fr;
		gap: var(--sp-4);
		margin-bottom: var(--sp-4);
	}

	.form-body {
		padding: var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.pair-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--sp-3);
	}

	/* ─── Fields ─── */
	.field { display: flex; flex-direction: column; gap: 6px; }

	.label-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.field label {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.label-aux { font-size: 0.76rem; color: var(--fg-muted); }
	.label-aux.ok { color: var(--green); font-weight: 500; }
	.label-aux.warn { color: var(--yellow); font-weight: 500; }

	/* Select */
	.select-wrap {
		position: relative;
		display: flex;
		align-items: center;
	}

	.select-icon {
		position: absolute;
		left: 10px;
		display: inline-flex;
		pointer-events: none;
		z-index: 1;
	}
	.select-icon :global(svg) { border-radius: 50%; }

	.select-chev {
		position: absolute;
		right: 10px;
		color: var(--fg-dim);
		display: inline-flex;
		pointer-events: none;
	}

	.select-wrap select {
		appearance: none;
		-webkit-appearance: none;
		width: 100%;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.88rem;
		padding: 10px 32px 10px 36px;
		outline: none;
		cursor: pointer;
		transition: border-color 0.15s, box-shadow 0.15s;
	}

	.select-wrap select:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
	}

	/* Amount input */
	.amount-wrap {
		position: relative;
		display: flex;
		align-items: center;
	}

	.amount-input {
		flex: 1;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg-strong);
		font-size: 1.2rem;
		font-weight: 600;
		padding: 12px 70px 12px 14px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}

	.amount-input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}

	.amount-suffix {
		position: absolute;
		right: 14px;
		color: var(--fg-muted);
		font-size: 0.85rem;
		font-weight: 600;
		pointer-events: none;
	}

	/* Address input */
	.addr-input {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.85rem;
		padding: 10px 12px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
		word-break: break-all;
	}
	.addr-input:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}

	/* ─── Tier panel ─── */
	.tier-panel {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		padding: var(--sp-3) var(--sp-4);
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.tier-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.tier-label {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.tier-badge {
		font-size: 0.74rem;
		font-weight: 600;
		padding: 3px 10px;
		border-radius: var(--r-full);
		letter-spacing: 0.01em;
	}
	.tier-badge.tone-ok { background: var(--green-soft); color: var(--green); border: 1px solid rgba(52, 211, 153, 0.3); }
	.tier-badge.tone-warn { background: var(--yellow-soft); color: var(--yellow); border: 1px solid rgba(245, 185, 66, 0.3); }
	.tier-badge.tone-crit { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.3); }

	.tier-body {
		display: grid;
		grid-template-columns: 1.5fr 1fr;
		gap: var(--sp-3);
	}
	.tier-body > div { display: flex; flex-direction: column; gap: 2px; }

	.tier-meta-label {
		font-size: 0.68rem;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-weight: 600;
	}

	.tier-meta-val {
		font-size: 0.84rem;
		color: var(--fg-strong);
		font-weight: 500;
	}

	.tier-bar {
		display: flex;
		gap: 3px;
		height: 4px;
	}

	.tier-seg {
		flex: 1;
		background: var(--surface-3);
		border-radius: 2px;
		transition: background 0.2s;
	}
	.tier-seg.active.tone-ok { background: var(--green); }
	.tier-seg.active.tone-warn { background: var(--yellow); }
	.tier-seg.active.tone-crit { background: var(--red); }

	/* ─── Fee panel ─── */
	.fee-panel {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		padding: var(--sp-3) var(--sp-4);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.fee-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 3px 0;
		font-size: 0.85rem;
	}

	.fee-label { color: var(--fg-muted); }
	.fee-highlight { color: var(--yellow); }
	.strong { color: var(--fg-strong); font-weight: 600; }

	.fee-row.total {
		border-top: 1px solid var(--border);
		margin-top: 6px;
		padding-top: 10px;
		font-size: 0.95rem;
	}

	/* ─── Alert ─── */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
	}
	.alert.inline { padding: 8px 10px; font-size: 0.8rem; }
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.alert.success {
		background: var(--green-soft);
		color: var(--green);
		border: 1px solid rgba(52, 211, 153, 0.25);
	}
	.alert > div { display: flex; flex-direction: column; gap: 2px; }
	.alert-title { font-weight: 600; }
	.alert-sub { font-size: 0.8rem; opacity: 0.8; }

	/* ─── Buttons ─── */
	.form-actions {
		display: flex;
		gap: var(--sp-2);
		padding-top: var(--sp-1);
	}

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 10px 16px;
		font-size: 0.88rem;
		font-weight: 550;
		border-radius: var(--r-md);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s, opacity 0.15s;
	}
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); color: var(--fg-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-secondary { background: var(--surface-2); }

	.btn-primary {
		background: var(--accent);
		color: #0A0B0D;
		border-color: var(--accent);
		font-weight: 600;
		margin-left: auto;
	}
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }

	.spinner {
		width: 13px; height: 13px;
		border: 2px solid #0A0B0D;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.spinner.sm { width: 11px; height: 11px; border-width: 2px; border-color: var(--accent); border-top-color: transparent; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ─── Info sidebar ─── */
	.info-panel { display: flex; flex-direction: column; }

	.flow-steps {
		list-style: none;
		padding: var(--sp-4) var(--sp-5);
		margin: 0;
		flex: 1;
		position: relative;
	}

	.flow-steps li {
		position: relative;
		display: flex;
		gap: 12px;
		padding: 0 0 var(--sp-4);
	}
	.flow-steps li:last-child { padding-bottom: 0; }

	.flow-steps li::before {
		content: '';
		position: absolute;
		left: 5px;
		top: 14px;
		bottom: 0;
		width: 1px;
		background: var(--border);
	}
	.flow-steps li:last-child::before { display: none; }

	.step-dot {
		width: 11px; height: 11px;
		border-radius: 50%;
		background: var(--surface);
		border: 2px solid var(--border-strong);
		flex-shrink: 0;
		margin-top: 4px;
		z-index: 1;
	}
	.flow-steps li.active .step-dot {
		border-color: var(--accent);
		background: var(--accent);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}

	.flow-steps strong {
		display: block;
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--fg-strong);
		margin-bottom: 3px;
	}
	.flow-steps p {
		margin: 0;
		font-size: 0.78rem;
		color: var(--fg-muted);
		line-height: 1.4;
	}

	.info-footer {
		padding: var(--sp-4) var(--sp-5);
		border-top: 1px solid var(--border);
		background: var(--surface-2);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.info-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.76rem;
		color: var(--fg-muted);
	}
	.info-meta > span:first-child { color: var(--accent); display: inline-flex; flex-shrink: 0; }

	/* ─── Data table ─── */
	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}

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
	.data-table tbody tr:hover { background: var(--hover-bg); }

	.data-table th.num, .data-table td.num { text-align: right; }
	.muted { color: var(--fg-muted); }
	.capitalize { text-transform: capitalize; }

	.status-pill {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: capitalize;
		padding: 2px 8px;
		border-radius: var(--r-full);
	}
	.status-pill.success { background: var(--green-soft); color: var(--green); }
	.status-pill.pending { background: var(--yellow-soft); color: var(--yellow); }
	.status-pill.danger  { background: var(--red-soft); color: var(--red); }
	.status-pill.muted   { background: var(--surface-2); color: var(--fg-muted); }

	.panel-body { padding: var(--sp-5); }

	.skel-line {
		height: 16px;
		width: 100%;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 8px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

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

	/* ─── Responsive ─── */
	@media (max-width: 1100px) {
		.withdraw-grid { grid-template-columns: 1fr; }
	}

	@media (max-width: 700px) {
		.pair-row { grid-template-columns: 1fr; }
		.tier-body { grid-template-columns: 1fr; }
		.form-actions { flex-direction: column; }
		.btn-primary { margin-left: 0; width: 100%; justify-content: center; }
		.btn-secondary { width: 100%; justify-content: center; }
	}
</style>
