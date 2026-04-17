<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon, tokenIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	interface DepositAddress {
		address: string;
		chain: string;
		memo?: string;
		qr_data: string;
		derivation_path?: string;
	}

	interface DepositRecord {
		id: string;
		asset: string;
		amount: string;
		confirmations: number;
		required_confirmations: number;
		status: string;
		tx_hash: string;
		created_at: string;
	}

	type ChainMeta = {
		id: string;
		label: string;
		native: string;
		tokens: string[];
		confirmations: number;
		arrivalMin: number;
		requiresMemo?: boolean;
	};

	const CHAINS: ChainMeta[] = [
		{ id: 'ethereum', label: 'Ethereum', native: 'ETH', tokens: ['ETH','USDT','USDC','DAI','WBTC'], confirmations: 12, arrivalMin: 3 },
		{ id: 'bitcoin',  label: 'Bitcoin',  native: 'BTC', tokens: ['BTC'],                           confirmations: 3,  arrivalMin: 30 },
		{ id: 'solana',   label: 'Solana',   native: 'SOL', tokens: ['SOL','USDT','USDC'],             confirmations: 32, arrivalMin: 1 },
		{ id: 'tron',     label: 'Tron',     native: 'TRX', tokens: ['TRX','USDT'],                    confirmations: 20, arrivalMin: 1 },
	];

	const UPCOMING_CHAINS: Array<{ id: string; label: string }> = [
		{ id: 'arbitrum', label: 'Arbitrum' },
		{ id: 'optimism', label: 'Optimism' },
		{ id: 'base',     label: 'Base' },
		{ id: 'polygon',  label: 'Polygon' },
		{ id: 'avalanche',label: 'Avalanche' },
		{ id: 'bsc',      label: 'BSC' },
	];

	let selectedChain = $state<ChainMeta>(CHAINS[0]);
	let addrInfo      = $state<DepositAddress | null>(null);
	let addrLoading   = $state(false);
	let addrError     = $state('');
	let copied        = $state(false);
	let qrSvg         = $state('');

	let history     = $state<DepositRecord[]>([]);
	let histLoading = $state(true);
	let histError   = $state('');

	let scannerStatus = $state<Record<string, string>>({});

	async function fetchAddress(chain: ChainMeta) {
		addrLoading = true;
		addrError   = '';
		addrInfo    = null;
		qrSvg       = '';
		try {
			addrInfo = await api.getJson<DepositAddress>(`/v1/user/deposit-address/${chain.id}`);
			try {
				qrSvg = await api.get(`/v1/user/deposit-address/${chain.id}/qr`);
			} catch { qrSvg = ''; }
		} catch (e) {
			addrError = e instanceof Error ? e.message : 'Failed to fetch deposit address';
		} finally {
			addrLoading = false;
		}
	}

	async function fetchHistory() {
		histLoading = true;
		histError   = '';
		try {
			const data = await api.getJson<{ deposits: DepositRecord[] }>('/v1/user/deposit-history');
			history = data.deposits ?? [];
		} catch (e) {
			histError = e instanceof Error ? e.message : 'Failed to load deposit history';
		} finally {
			histLoading = false;
		}
	}

	async function fetchScannerStatus() {
		try {
			const h = await api.getJson<{ chains: Record<string, { status: string }> }>('/health/chains');
			const result: Record<string, string> = {};
			for (const [k, v] of Object.entries(h.chains ?? {})) result[k] = v.status;
			scannerStatus = result;
		} catch { /* ignore */ }
	}

	async function copyAddress() {
		if (!addrInfo) return;
		await navigator.clipboard.writeText(addrInfo.address);
		copied = true;
		setTimeout(() => { copied = false; }, 1800);
	}

	async function copyMemo() {
		if (!addrInfo?.memo) return;
		await navigator.clipboard.writeText(addrInfo.memo);
	}

	function selectChain(c: ChainMeta) {
		selectedChain = c;
		fetchAddress(c);
	}

	onMount(() => {
		fetchAddress(selectedChain);
		fetchHistory();
		fetchScannerStatus();
	});

	function statusTone(s: string): 'success' | 'pending' | 'danger' | 'muted' {
		if (s === 'confirmed') return 'success';
		if (s === 'pending' || s === 'broadcast') return 'pending';
		if (s === 'failed') return 'danger';
		return 'muted';
	}

	function scannerTone(id: string): 'ok' | 'warn' | 'err' | 'unknown' {
		const s = scannerStatus[id];
		if (s === 'healthy' || s === 'ok') return 'ok';
		if (s === 'degraded') return 'warn';
		if (s === 'down') return 'err';
		return 'unknown';
	}

	function scannerLabel(id: string): string {
		const s = scannerStatus[id];
		if (s === 'healthy' || s === 'ok') return 'Scanner online';
		if (s === 'degraded') return 'Degraded';
		if (s === 'down') return 'Offline';
		return 'Unknown';
	}

	function trunc(s: string, pre = 10, post = 8): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function fmtDate(iso: string): string {
		const d = new Date(iso);
		return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
	}
</script>

<!-- ════════════════════ Header ════════════════════ -->
<div class="page-head">
	<div>
		<h1>Deposit</h1>
		<p class="page-sub">Generate a deterministic address for any supported network. Transfers are credited automatically after confirmations.</p>
	</div>
</div>

<!-- ════════════════════ Chain selector ════════════════════ -->
<section class="chain-selector">
	{#each CHAINS as c}
		<button
			class="chain-pill"
			class:active={selectedChain.id === c.id}
			onclick={() => selectChain(c)}
		>
			<span class="chain-pill-icon">{@html chainIcon(c.id, 16)}</span>
			<span class="chain-pill-label">{c.label}</span>
			<span class="chain-pill-dot dot-{scannerTone(c.id)}"></span>
		</button>
	{/each}
	<div class="chain-divider" aria-hidden="true"></div>
	{#each UPCOMING_CHAINS as c}
		<button class="chain-pill soon" disabled title="Coming soon">
			<span class="chain-pill-icon">{@html chainIcon(c.id, 16)}</span>
			<span class="chain-pill-label">{c.label}</span>
			<span class="soon-tag">Soon</span>
		</button>
	{/each}
</section>

<!-- ════════════════════ Primary panel ════════════════════ -->
<div class="deposit-grid">
	<!-- QR + address -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>
					<span class="chain-title-icon">{@html chainIcon(selectedChain.id, 18)}</span>
					<span>{selectedChain.label}</span>
					<span class="network-tag">Mainnet</span>
				</h2>
				<p class="panel-sub">Deterministic address derived from your workspace HD key</p>
			</div>
			<span class="scanner-chip tone-{scannerTone(selectedChain.id)}">
				<span class="dot"></span>
				{scannerLabel(selectedChain.id)}
			</span>
		</header>

		<div class="deposit-body">
			{#if addrLoading}
				<div class="qr-skeleton">
					<div class="qr-box-skel"></div>
					<div class="skel-line"></div>
					<div class="skel-line short"></div>
				</div>
			{:else if addrError}
				<div class="alert danger">
					<span>{@html icon('alert-triangle', 16)}</span>
					<span>{addrError}</span>
					<button class="btn btn-ghost sm" onclick={() => fetchAddress(selectedChain)}>Retry</button>
				</div>
			{:else if addrInfo}
				<div class="addr-layout">
					<!-- QR -->
					<div class="qr-stage">
						<div class="qr-frame">
							{#if qrSvg}
								<div class="qr-inner">{@html qrSvg}</div>
							{:else}
								<div class="qr-fallback">
									<span class="qr-fallback-icon">{@html icon('qr-code', 56)}</span>
									<span class="qr-fallback-txt">QR unavailable — copy address below</span>
								</div>
							{/if}
						</div>
						<span class="qr-caption">Scan with any compatible wallet</span>
					</div>

					<!-- Details -->
					<div class="addr-details">
						<div class="addr-block">
							<div class="field-label-row">
								<span class="field-label">Deposit address</span>
								<span class="addr-net">{selectedChain.label}</span>
							</div>
							<div class="addr-card">
								<code class="addr-text mono">{addrInfo.address}</code>
								<button class="copy-btn" onclick={copyAddress}>
									{#if copied}
										<span class="copy-icon">{@html icon('check', 14)}</span>
										<span>Copied</span>
									{:else}
										<span class="copy-icon">{@html icon('copy', 14)}</span>
										<span>Copy</span>
									{/if}
								</button>
							</div>
						</div>

						{#if addrInfo.memo}
							<div class="addr-block">
								<div class="field-label-row">
									<span class="field-label required">Memo / Destination tag</span>
									<span class="required-badge">Required</span>
								</div>
								<div class="addr-card memo">
									<code class="addr-text mono">{addrInfo.memo}</code>
									<button class="copy-btn" onclick={copyMemo}>
										<span class="copy-icon">{@html icon('copy', 14)}</span>
										<span>Copy</span>
									</button>
								</div>
							</div>
						{/if}

						<!-- Metadata -->
						<dl class="meta-grid">
							<div>
								<dt>Required confirmations</dt>
								<dd class="mono">{selectedChain.confirmations}</dd>
							</div>
							<div>
								<dt>Estimated arrival</dt>
								<dd class="mono">~{selectedChain.arrivalMin}m</dd>
							</div>
							{#if addrInfo.derivation_path}
								<div class="meta-wide">
									<dt>Derivation path</dt>
									<dd class="mono">{addrInfo.derivation_path}</dd>
								</div>
							{/if}
						</dl>

						<!-- Supported assets -->
						<div class="assets-strip">
							<span class="field-label">Accepted on this address</span>
							<div class="assets-chips">
								{#each selectedChain.tokens as tok}
									<span class="asset-chip">
										{@html tokenIcon(tok, 14)}
										<span>{tok}</span>
									</span>
								{/each}
							</div>
						</div>
					</div>
				</div>

				<!-- Warning bar -->
				<div class="warn-bar">
					<span class="warn-icon">{@html icon('alert-triangle', 16)}</span>
					<div class="warn-body">
						<span class="warn-title">Send only {selectedChain.tokens.join(', ')} on the {selectedChain.label} network.</span>
						<span class="warn-sub">Transfers of other assets or on other networks are unrecoverable.</span>
					</div>
				</div>
			{/if}
		</div>
	</section>

	<!-- Instructions panel -->
	<aside class="panel instructions">
		<header class="panel-head">
			<h2>
				<span class="instr-icon">{@html icon('file-text', 14)}</span>
				How it works
			</h2>
		</header>

		<ol class="instr-steps">
			<li>
				<span class="step-num">1</span>
				<div>
					<strong>Copy address</strong>
					<p>Send from any exchange or wallet. Double-check the network matches.</p>
				</div>
			</li>
			<li>
				<span class="step-num">2</span>
				<div>
					<strong>Confirmations accumulate</strong>
					<p>Scanner tracks the tx in real time. Status updates at each block.</p>
				</div>
			</li>
			<li>
				<span class="step-num">3</span>
				<div>
					<strong>Credit posted</strong>
					<p>After {selectedChain.confirmations} confirmations, balance is credited and double-entry booked.</p>
				</div>
			</li>
		</ol>

		<div class="instr-footer">
			<div class="instr-meta">
				<span>{@html icon('shield-check', 12)}</span>
				<span>Address is deterministic — regenerable from HD seed</span>
			</div>
			<div class="instr-meta">
				<span>{@html icon('radio', 12)}</span>
				<span>Deposits monitored continuously — no manual action required</span>
			</div>
		</div>
	</aside>
</div>

<!-- ════════════════════ History ════════════════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Deposit history</h2>
			<p class="panel-sub">All incoming transfers across networks</p>
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
			<span class="empty-mark">{@html icon('arrow-down-left', 22)}</span>
			<div>
				<p class="empty-title">No deposits received yet</p>
				<p class="muted">Incoming transfers will appear here the moment they're detected on-chain.</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>Asset</th>
					<th class="num">Amount</th>
					<th>Transaction</th>
					<th>Confirmations</th>
					<th>Status</th>
					<th class="num">When</th>
				</tr>
			</thead>
			<tbody>
				{#each history as dep}
					{@const pct = Math.min(100, (dep.confirmations / dep.required_confirmations) * 100)}
					<tr>
						<td>
							<div class="token-cell">
								{@html tokenIcon(dep.asset, 18)}
								<span class="strong">{dep.asset}</span>
							</div>
						</td>
						<td class="num mono strong">{dep.amount}</td>
						<td class="mono muted">{trunc(dep.tx_hash)}</td>
						<td>
							<div class="confirm-cell">
								<div class="confirm-bar">
									<div class="confirm-fill" style="width:{pct}%"></div>
								</div>
								<span class="mono confirm-txt">{dep.confirmations}/{dep.required_confirmations}</span>
							</div>
						</td>
						<td><span class="status-pill {statusTone(dep.status)}">{dep.status}</span></td>
						<td class="num mono muted">{fmtDate(dep.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
	/* ═══ Page head ═══ */
	.page-head { margin-bottom: var(--sp-5); }
	h1 {
		margin: 0 0 4px;
		font-size: 1.5rem;
		font-weight: 650;
		letter-spacing: -0.022em;
	}
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	/* ═══ Chain selector ═══ */
	.chain-selector {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		padding: 6px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		margin-bottom: var(--sp-4);
	}

	.chain-pill {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 7px 12px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		font-size: 0.84rem;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s, color 0.15s;
	}
	.chain-pill:hover { background: var(--hover-bg); color: var(--fg); }
	.chain-pill.active {
		background: var(--accent-soft);
		border-color: var(--accent-line);
		color: var(--fg-strong);
	}

	.chain-pill-icon { display: inline-flex; }
	.chain-pill-icon :global(svg) { border-radius: 50%; }

	.chain-pill-dot { width: 6px; height: 6px; border-radius: 50%; }

	.chain-pill.soon {
		cursor: not-allowed;
		opacity: 0.55;
	}
	.chain-pill.soon:hover { background: transparent; color: var(--fg-muted); }

	.soon-tag {
		font-size: 0.62rem;
		font-weight: 700;
		padding: 1px 6px;
		background: var(--surface-2);
		color: var(--fg-dim);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.chain-divider {
		width: 1px;
		align-self: stretch;
		background: var(--border);
		margin: 0 2px;
	}
	.dot-ok { background: var(--green); box-shadow: 0 0 0 2px rgba(52, 211, 153, 0.18); }
	.dot-warn { background: var(--yellow); box-shadow: 0 0 0 2px rgba(245, 185, 66, 0.18); }
	.dot-err { background: var(--red); box-shadow: 0 0 0 2px rgba(248, 113, 113, 0.18); }
	.dot-unknown { background: var(--fg-dim); }

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

	.panel-head h2 {
		margin: 0 0 2px;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.chain-title-icon { display: inline-flex; }
	.chain-title-icon :global(svg) { border-radius: 50%; }

	.network-tag {
		font-size: 0.66rem;
		font-weight: 600;
		padding: 2px 7px;
		background: var(--surface-2);
		color: var(--fg-muted);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }

	.scanner-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 10px;
		font-size: 0.72rem;
		font-weight: 500;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
		background: var(--surface-2);
	}
	.scanner-chip .dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
	.scanner-chip.tone-ok { color: var(--green); border-color: rgba(52, 211, 153, 0.3); background: var(--green-soft); }
	.scanner-chip.tone-warn { color: var(--yellow); border-color: rgba(245, 185, 66, 0.3); background: var(--yellow-soft); }
	.scanner-chip.tone-err { color: var(--red); border-color: rgba(248, 113, 113, 0.3); background: var(--red-soft); }
	.scanner-chip.tone-unknown { color: var(--fg-muted); }

	/* ═══ Deposit grid ═══ */
	.deposit-grid {
		display: grid;
		grid-template-columns: 1.6fr 1fr;
		gap: var(--sp-4);
		margin-bottom: var(--sp-4);
	}

	.deposit-body { padding: 0; }

	.addr-layout {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: var(--sp-6);
		padding: var(--sp-5);
	}

	/* ═══ QR stage ═══ */
	.qr-stage {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-3);
	}

	.qr-frame {
		padding: 12px;
		background: #fff;
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		box-shadow: var(--shadow-1);
	}

	.qr-inner { width: 176px; height: 176px; }
	.qr-inner :global(svg) { width: 100%; height: 100%; display: block; }

	.qr-fallback {
		width: 176px; height: 176px;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		color: var(--fg-dim);
		background: #fafafa;
		border-radius: 4px;
	}
	.qr-fallback-icon { color: #d0d5dd; }
	.qr-fallback-txt { font-size: 0.72rem; color: #667085; text-align: center; }

	.qr-caption { font-size: 0.72rem; color: var(--fg-muted); }

	/* ═══ Address details ═══ */
	.addr-details { display: flex; flex-direction: column; gap: var(--sp-4); min-width: 0; }

	.addr-block { display: flex; flex-direction: column; gap: 6px; }

	.field-label-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }

	.field-label {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.field-label.required { color: var(--yellow); }

	.addr-net {
		font-size: 0.72rem;
		color: var(--fg-muted);
		text-transform: capitalize;
	}

	.required-badge {
		font-size: 0.65rem;
		font-weight: 700;
		background: var(--yellow-soft);
		color: var(--yellow);
		padding: 2px 8px;
		border-radius: var(--r-full);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.addr-card {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 10px 10px 14px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.addr-card.memo {
		border-color: rgba(245, 185, 66, 0.35);
		background: var(--yellow-soft);
	}

	.addr-text {
		flex: 1;
		font-size: 0.82rem;
		color: var(--fg-strong);
		word-break: break-all;
		line-height: 1.5;
	}

	.copy-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.78rem;
		font-weight: 500;
		cursor: pointer;
		flex-shrink: 0;
		transition: background 0.15s, border-color 0.15s, color 0.15s;
	}
	.copy-btn:hover { background: var(--hover-bg); border-color: var(--border-strong); color: var(--fg-strong); }
	.copy-btn .copy-icon { display: inline-flex; color: var(--fg-dim); }
	.copy-btn:hover .copy-icon { color: var(--accent); }

	/* ═══ Meta grid ═══ */
	.meta-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--sp-3);
		margin: 0;
		padding: var(--sp-3) 0;
		border-top: 1px solid var(--border);
		border-bottom: 1px solid var(--border);
	}

	.meta-grid > div { display: flex; flex-direction: column; gap: 2px; }
	.meta-grid .meta-wide { grid-column: span 2; }

	.meta-grid dt {
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.meta-grid dd {
		margin: 0;
		font-size: 0.86rem;
		color: var(--fg-strong);
		font-weight: 500;
	}

	/* ═══ Assets strip ═══ */
	.assets-strip { display: flex; flex-direction: column; gap: 8px; }

	.assets-chips { display: flex; flex-wrap: wrap; gap: 6px; }

	.asset-chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 4px 9px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		font-size: 0.76rem;
		font-weight: 500;
		color: var(--fg);
	}
	.asset-chip :global(svg) { border-radius: 50%; }

	/* ═══ Warn bar ═══ */
	.warn-bar {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: var(--sp-4) var(--sp-5);
		background: var(--red-soft);
		border-top: 1px solid rgba(248, 113, 113, 0.2);
		color: var(--red);
	}
	.warn-icon { display: inline-flex; flex-shrink: 0; margin-top: 1px; }
	.warn-body { display: flex; flex-direction: column; gap: 2px; }
	.warn-title { font-weight: 600; font-size: 0.84rem; }
	.warn-sub { font-size: 0.78rem; opacity: 0.85; }

	/* ═══ Instructions panel ═══ */
	.instructions { display: flex; flex-direction: column; }

	.instr-icon { display: inline-flex; color: var(--fg-muted); }

	.instr-steps {
		list-style: none;
		padding: var(--sp-4) var(--sp-5);
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		flex: 1;
	}

	.instr-steps li { display: flex; gap: 12px; align-items: flex-start; }

	.step-num {
		width: 22px;
		height: 22px;
		border-radius: 50%;
		background: var(--accent-soft);
		color: var(--accent);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 0.72rem;
		font-weight: 700;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.instr-steps strong {
		display: block;
		font-size: 0.86rem;
		font-weight: 600;
		color: var(--fg-strong);
		margin-bottom: 2px;
	}

	.instr-steps p {
		margin: 0;
		font-size: 0.8rem;
		color: var(--fg-muted);
		line-height: 1.4;
	}

	.instr-footer {
		padding: var(--sp-4) var(--sp-5);
		border-top: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		gap: 8px;
		background: var(--surface-2);
	}

	.instr-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.76rem;
		color: var(--fg-muted);
	}
	.instr-meta span:first-child { color: var(--accent); display: inline-flex; flex-shrink: 0; }

	/* ═══ Data table (shared) ═══ */
	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

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

	.data-table td .strong, .strong { color: var(--fg-strong); font-weight: 500; }
	.muted { color: var(--fg-muted); }

	.confirm-cell { display: flex; align-items: center; gap: 8px; min-width: 140px; }
	.confirm-bar {
		flex: 1;
		max-width: 80px;
		height: 5px;
		background: var(--surface-3);
		border-radius: 999px;
		overflow: hidden;
	}
	.confirm-fill {
		height: 100%;
		background: linear-gradient(90deg, var(--accent) 0%, var(--accent-strong) 100%);
		border-radius: 999px;
	}
	.confirm-txt { font-size: 0.76rem; color: var(--fg-muted); white-space: nowrap; }

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

	/* ═══ Empty ═══ */
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

	/* ═══ Alert ═══ */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin: var(--sp-5);
	}
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.btn { display: inline-flex; align-items: center; gap: 6px; padding: 7px 12px; font-size: 0.84rem; font-weight: 550; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--surface-2); color: var(--fg); cursor: pointer; text-decoration: none; }
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	/* ═══ Skeletons ═══ */
	.qr-skeleton {
		padding: var(--sp-5);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--sp-3);
	}
	.qr-box-skel {
		width: 200px; height: 200px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-md);
	}
	.skel-line {
		height: 16px;
		width: 100%;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 8px;
	}
	.skel-line.short { width: 60%; }
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	/* ═══ Responsive ═══ */
	@media (max-width: 1100px) {
		.deposit-grid { grid-template-columns: 1fr; }
	}

	@media (max-width: 800px) {
		.addr-layout { grid-template-columns: 1fr; }
		.qr-stage { align-items: flex-start; }
		.meta-grid { grid-template-columns: 1fr; }
		.meta-grid .meta-wide { grid-column: span 1; }
	}
</style>
