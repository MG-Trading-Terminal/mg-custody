<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api';
	import { tokenIcon, chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	interface Vault { id: string; name: string; }

	interface Approval {
		id: string;
		withdrawal_id: string;
		asset: string;
		chain: string;
		amount: string;
		destination: string;
		requester: string;
		tier: number;
		approvals_given: number;
		approvals_required: number;
		expires_at: string;
		created_at: string;
		usd_value?: string;
	}

	let vaults    = $state<Vault[]>([]);
	let vaultId   = $state('');
	let approvals = $state<Approval[]>([]);
	let loading   = $state(false);
	let error     = $state('');
	let acting    = $state<Record<string, 'approve' | 'reject' | null>>({});
	let selected  = $state<Approval | null>(null);
	let now       = $state(Date.now());

	async function loadVaults() {
		try {
			const d = await api.getJson<{ vaults: Vault[] }>('/v1/vaults');
			vaults = d.vaults ?? [];
			if (vaults.length > 0 && !vaultId) {
				vaultId = vaults[0].id;
				await loadApprovals();
			}
		} catch { /* ignore */ }
	}

	async function loadApprovals() {
		if (!vaultId) return;
		loading = true;
		error = '';
		try {
			const d = await api.getJson<{ approvals: Approval[] }>(`/v1/vaults/${vaultId}/approvals`);
			approvals = d.approvals ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load approvals';
		} finally {
			loading = false;
		}
	}

	async function act(id: string, action: 'approve' | 'reject') {
		acting = { ...acting, [id]: action };
		try {
			await api.post(`/v1/approvals/${id}/${action}`);
			await loadApprovals();
			if (selected?.id === id) selected = null;
		} catch (e) {
			error = e instanceof Error ? e.message : `Failed to ${action}`;
		} finally {
			acting = { ...acting, [id]: null };
		}
	}

	let timerId: ReturnType<typeof setInterval>;
	onMount(() => {
		loadVaults();
		timerId = setInterval(() => { now = Date.now(); }, 1000);
	});
	onDestroy(() => clearInterval(timerId));

	function trunc(s: string, pre = 10, post = 8): string {
		if (!s) return '';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function timeLeft(iso: string): { label: string; tone: 'ok' | 'warn' | 'crit' | 'expired' } {
		const diff = new Date(iso).getTime() - now;
		if (diff <= 0) return { label: 'Expired', tone: 'expired' };
		const h = Math.floor(diff / 3_600_000);
		const m = Math.floor((diff % 3_600_000) / 60_000);
		const s = Math.floor((diff % 60_000) / 1000);
		const label = h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${s}s` : `${s}s`;
		const tone = diff < 5 * 60_000 ? 'crit' : diff < 30 * 60_000 ? 'warn' : 'ok';
		return { label, tone };
	}

	function tierTone(tier: number): 'ok' | 'warn' | 'crit' {
		if (tier <= 1) return 'ok';
		if (tier <= 3) return 'warn';
		return 'crit';
	}

	function tierLabel(tier: number): string {
		return ['Tier 0 · Auto', 'Tier 1 · Auto', 'Tier 2', 'Tier 3', 'Tier 4'][tier] ?? `Tier ${tier}`;
	}

	function fmtUsd(v?: string): string {
		if (!v) return '';
		const n = Number(v);
		if (!isFinite(n)) return '';
		return `$${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
	}

	function copy(s: string) { navigator.clipboard.writeText(s); }

	let stats = $derived({
		tier2: approvals.filter(a => a.tier === 2).length,
		tier3: approvals.filter(a => a.tier === 3).length,
		tier4: approvals.filter(a => a.tier === 4).length,
		expiring: approvals.filter(a => {
			const diff = new Date(a.expires_at).getTime() - now;
			return diff > 0 && diff < 30 * 60_000;
		}).length,
	});
</script>

<!-- ═══════════ Header ═══════════ -->
<div class="page-head">
	<div>
		<h1>Approvals</h1>
		<p class="page-sub">Review and sign off on pending withdrawals. Tier 2+ requires FIDO2 hardware key.</p>
	</div>
	<div class="head-actions">
		{#if vaults.length > 0}
			<div class="vault-picker">
				<span class="vault-label">Vault</span>
				<select bind:value={vaultId} onchange={loadApprovals}>
					{#each vaults as v}<option value={v.id}>{v.name}</option>{/each}
				</select>
			</div>
		{/if}
		<button class="btn btn-ghost" onclick={loadApprovals} title="Refresh">
			<span>{@html icon('refresh', 14)}</span>
			<span>Refresh</span>
		</button>
	</div>
</div>

<!-- ═══════════ Stat strip ═══════════ -->
<section class="stat-strip">
	<div class="stat-tile">
		<span class="stat-label">Pending total</span>
		<span class="stat-value mono">{approvals.length}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Tier 2</span>
		<span class="stat-value mono tone-warn">{stats.tier2}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Tier 3</span>
		<span class="stat-value mono tone-warn">{stats.tier3}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Tier 4 · Cold</span>
		<span class="stat-value mono tone-crit">{stats.tier4}</span>
	</div>
	<div class="stat-tile">
		<span class="stat-label">Expiring &lt; 30m</span>
		<span class="stat-value mono" class:tone-crit={stats.expiring > 0}>{stats.expiring}</span>
	</div>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
	</div>
{/if}

<!-- ═══════════ Queue ═══════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Approval queue</h2>
			<p class="panel-sub">Withdrawals awaiting authorization</p>
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			{#each Array(3) as _}
				<div class="skel-card"></div>
			{/each}
		</div>
	{:else if vaults.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('wallets', 22)}</span>
			<div>
				<p class="empty-title">No vaults configured</p>
				<p class="muted">Create a vault first — approvals are scoped per vault.</p>
			</div>
		</div>
	{:else if approvals.length === 0}
		<div class="panel-empty">
			<span class="empty-mark ok">{@html icon('check', 22)}</span>
			<div>
				<p class="empty-title">Queue is clear</p>
				<p class="muted">All withdrawals have been processed. Tier 2+ withdrawals will appear here.</p>
			</div>
		</div>
	{:else}
		<ul class="approval-list">
			{#each approvals as a}
				{@const tl = timeLeft(a.expires_at)}
				{@const prog = a.approvals_required > 0 ? (a.approvals_given / a.approvals_required) * 100 : 0}
				{@const isBusy = !!acting[a.id]}
				<li class="approval-card" class:expired={tl.tone === 'expired'}>
					<button class="card-click" onclick={() => selected = a} aria-label="View details">
						<div class="card-row">
							<!-- Asset -->
							<div class="card-asset">
								{@html tokenIcon(a.asset, 28)}
								<div>
									<span class="mono amount-big">{a.amount} <span class="amount-sym">{a.asset}</span></span>
									<span class="mono muted tiny">{a.usd_value ? fmtUsd(a.usd_value) : ''}</span>
								</div>
							</div>

							<!-- Destination + chain -->
							<div class="card-dest">
								<span class="dest-label">To</span>
								<span class="mono dest-addr">{trunc(a.destination, 12, 10)}</span>
								<span class="chain-chip">
									{@html chainIcon(a.chain, 12)}
									<span class="capitalize">{a.chain}</span>
								</span>
							</div>

							<!-- Tier -->
							<div class="card-tier">
								<span class="tier-badge tone-{tierTone(a.tier)}">{tierLabel(a.tier)}</span>
								<div class="progress">
									<div class="progress-bar">
										<div class="progress-fill" style="width:{prog}%"></div>
									</div>
									<span class="mono progress-txt">{a.approvals_given}/{a.approvals_required}</span>
								</div>
							</div>

							<!-- Expires countdown -->
							<div class="card-time">
								<span class="tiny muted">Expires in</span>
								<span class="mono countdown tone-{tl.tone}">{tl.label}</span>
							</div>
						</div>
					</button>

					<!-- Actions -->
					<div class="card-actions">
						<button
							class="btn btn-reject"
							disabled={isBusy || tl.tone === 'expired'}
							onclick={() => act(a.id, 'reject')}
						>
							{#if acting[a.id] === 'reject'}
								<span class="spinner sm"></span>
								<span>Rejecting…</span>
							{:else}
								<span>{@html icon('x', 14)}</span>
								<span>Reject</span>
							{/if}
						</button>
						<button
							class="btn btn-approve"
							disabled={isBusy || tl.tone === 'expired'}
							onclick={() => act(a.id, 'approve')}
						>
							{#if acting[a.id] === 'approve'}
								<span class="spinner sm light"></span>
								<span>Signing…</span>
							{:else}
								{#if a.tier >= 2}
									<span>{@html icon('key', 14)}</span>
									<span>Approve · FIDO2</span>
								{:else}
									<span>{@html icon('check', 14)}</span>
									<span>Approve</span>
								{/if}
							{/if}
						</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<!-- ═══════════ Detail drawer ═══════════ -->
{#if selected}
	<div class="drawer-backdrop" role="presentation" onclick={() => selected = null} onkeydown={(e) => e.key === 'Escape' && (selected = null)}></div>
	<aside class="drawer">
		<header class="drawer-head">
			<div>
				<h3>Withdrawal approval</h3>
				<span class="mono muted">{trunc(selected.withdrawal_id, 14, 10)}</span>
			</div>
			<button class="icon-btn" onclick={() => selected = null}>{@html icon('x', 16)}</button>
		</header>

		<div class="drawer-amount">
			{@html tokenIcon(selected.asset, 32)}
			<span class="mono drawer-amt">{selected.amount}</span>
			<span class="drawer-sym">{selected.asset}</span>
			{#if selected.usd_value}
				<div class="mono muted drawer-usd">≈ {fmtUsd(selected.usd_value)}</div>
			{/if}
		</div>

		<div class="drawer-body">
			<span class="tier-badge tone-{tierTone(selected.tier)} block">{tierLabel(selected.tier)}</span>

			<dl class="detail-list">
				<div>
					<dt>Destination</dt>
					<dd>
						<div class="copyable">
							<code class="mono">{selected.destination}</code>
							<button class="copy-mini" onclick={() => copy(selected!.destination)}>{@html icon('copy', 12)}</button>
						</div>
					</dd>
				</div>
				<div>
					<dt>Network</dt>
					<dd class="capitalize">
						<span class="token-cell">{@html chainIcon(selected.chain, 14)} {selected.chain}</span>
					</dd>
				</div>
				<div>
					<dt>Requester</dt>
					<dd>{selected.requester}</dd>
				</div>
				<div>
					<dt>Signatures collected</dt>
					<dd class="mono">{selected.approvals_given} / {selected.approvals_required}</dd>
				</div>
				<div>
					<dt>Expires</dt>
					<dd>{timeLeft(selected.expires_at).label}</dd>
				</div>
			</dl>

			<div class="drawer-actions">
				<button class="btn btn-reject full" disabled={!!acting[selected.id]} onclick={() => act(selected!.id, 'reject')}>
					<span>{@html icon('x', 14)}</span>
					<span>Reject withdrawal</span>
				</button>
				<button class="btn btn-approve full" disabled={!!acting[selected.id]} onclick={() => act(selected!.id, 'approve')}>
					<span>{@html icon('key', 14)}</span>
					<span>{selected.tier >= 2 ? 'Approve with FIDO2' : 'Approve'}</span>
				</button>
			</div>
		</div>
	</aside>
{/if}

<style>
	/* ═════ Head ═════ */
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

	.head-actions { display: flex; gap: var(--sp-2); align-items: center; }

	.vault-picker {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 8px 4px 12px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.vault-label {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.vault-picker select {
		background: transparent;
		border: none;
		color: var(--fg);
		font-size: 0.85rem;
		padding: 4px 20px 4px 4px;
		cursor: pointer;
		outline: none;
		appearance: none;
		-webkit-appearance: none;
		background: transparent url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%238A919E' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='6 9 12 15 18 9'/></svg>") no-repeat right 2px center;
	}

	/* ═════ Stat strip ═════ */
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
		font-size: 1.5rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
		line-height: 1;
	}
	.stat-value.tone-warn { color: var(--yellow); }
	.stat-value.tone-crit { color: var(--red); }

	/* ═════ Panel ═════ */
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

	.panel-body { padding: var(--sp-4); display: flex; flex-direction: column; gap: 8px; }

	/* ═════ Alert ═════ */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}

	/* ═════ Buttons ═════ */
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		padding: 8px 14px;
		font-size: 0.85rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s, opacity 0.15s;
	}
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn.full { width: 100%; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.btn-approve {
		background: var(--green);
		color: #0A0B0D;
		border-color: var(--green);
		font-weight: 600;
	}
	.btn-approve:hover:not(:disabled) { background: #2fbb86; border-color: #2fbb86; }

	.btn-reject {
		background: transparent;
		border-color: rgba(248, 113, 113, 0.35);
		color: var(--red);
	}
	.btn-reject:hover:not(:disabled) { background: var(--red-soft); border-color: var(--red); }

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
		border: 2px solid var(--fg-muted);
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.spinner.sm { width: 11px; height: 11px; }
	.spinner.light { border-color: #0A0B0D; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ═════ Approval cards ═════ */
	.approval-list {
		list-style: none;
		padding: var(--sp-3) var(--sp-4);
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.approval-card {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-3) var(--sp-4);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		transition: border-color 0.15s, background 0.15s;
	}
	.approval-card:hover { border-color: var(--border-strong); }
	.approval-card.expired { opacity: 0.55; }

	.card-click {
		flex: 1;
		display: flex;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		text-align: left;
		color: inherit;
	}

	.card-row {
		display: grid;
		grid-template-columns: 1.1fr 1.3fr 1fr 0.8fr;
		gap: var(--sp-4);
		width: 100%;
		align-items: center;
	}

	.card-asset { display: flex; align-items: center; gap: 10px; }
	.card-asset :global(svg) { border-radius: 50%; }
	.card-asset > div { display: flex; flex-direction: column; gap: 2px; line-height: 1.1; }

	.amount-big {
		font-size: 1rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.01em;
	}
	.amount-sym { color: var(--fg-muted); font-weight: 500; font-size: 0.85rem; }

	.card-dest {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.dest-label {
		font-size: 0.66rem;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.1em;
		font-weight: 600;
	}
	.dest-addr { font-size: 0.84rem; color: var(--fg); word-break: break-all; }

	.chain-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.72rem;
		color: var(--fg-muted);
		margin-top: 1px;
	}
	.chain-chip :global(svg) { border-radius: 50%; }

	.card-tier { display: flex; flex-direction: column; gap: 6px; }

	.tier-badge {
		font-size: 0.7rem;
		font-weight: 600;
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid transparent;
		width: fit-content;
	}
	.tier-badge.block { display: block; margin-bottom: var(--sp-3); }
	.tier-badge.tone-ok { background: var(--green-soft); color: var(--green); border-color: rgba(52, 211, 153, 0.3); }
	.tier-badge.tone-warn { background: var(--yellow-soft); color: var(--yellow); border-color: rgba(245, 185, 66, 0.3); }
	.tier-badge.tone-crit { background: var(--red-soft); color: var(--red); border-color: rgba(248, 113, 113, 0.3); }

	.progress { display: flex; align-items: center; gap: 8px; }
	.progress-bar {
		flex: 1;
		max-width: 100px;
		height: 4px;
		background: var(--surface-3);
		border-radius: 2px;
		overflow: hidden;
	}
	.progress-fill {
		height: 100%;
		background: var(--accent);
		border-radius: 2px;
	}
	.progress-txt { font-size: 0.74rem; color: var(--fg-muted); }

	.card-time {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 2px;
	}
	.tiny { font-size: 0.68rem; }
	.countdown {
		font-size: 0.92rem;
		font-weight: 600;
		letter-spacing: -0.01em;
	}
	.countdown.tone-ok { color: var(--fg-strong); }
	.countdown.tone-warn { color: var(--yellow); }
	.countdown.tone-crit { color: var(--red); }
	.countdown.tone-expired { color: var(--fg-muted); }

	.card-actions {
		display: flex;
		gap: 8px;
		flex-shrink: 0;
	}

	.capitalize { text-transform: capitalize; }
	.muted { color: var(--fg-muted); }

	/* ═════ Empty & skel ═════ */
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
	.empty-mark.ok { background: var(--green-soft); color: var(--green); }
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-card {
		height: 68px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-md);
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	/* ═════ Drawer ═════ */
	.drawer-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(4px);
		z-index: 80;
		animation: fadeIn 0.15s;
	}
	@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

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
	.drawer-head h3 { margin: 0 0 2px; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }
	.drawer-head .muted { font-size: 0.76rem; }

	.drawer-amount {
		padding: var(--sp-6) var(--sp-5);
		text-align: center;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}
	.drawer-amount :global(svg) { border-radius: 50%; margin-bottom: 6px; }
	.drawer-amt {
		font-size: 2rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.025em;
	}
	.drawer-sym {
		font-size: 0.9rem;
		color: var(--fg-muted);
		font-weight: 500;
		margin-left: -4px;
	}
	.drawer-usd { font-size: 0.85rem; }

	.drawer-body {
		flex: 1;
		overflow-y: auto;
		padding: var(--sp-5);
	}

	.detail-list { margin: 0 0 var(--sp-4); padding: 0; }
	.detail-list > div {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: var(--sp-4);
		padding: var(--sp-3) 0;
		border-bottom: 1px solid var(--border);
	}
	.detail-list > div:last-child { border-bottom: none; }
	.detail-list dt {
		font-size: 0.76rem;
		color: var(--fg-muted);
		font-weight: 500;
		flex-shrink: 0;
		min-width: 130px;
	}
	.detail-list dd {
		margin: 0;
		font-size: 0.84rem;
		text-align: right;
		color: var(--fg);
		word-break: break-all;
	}

	.token-cell { display: inline-flex; align-items: center; gap: 6px; }
	.token-cell :global(svg) { border-radius: 50%; }

	.copyable {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 4px 8px;
	}
	.copyable code { font-size: 0.76rem; color: var(--fg); }
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

	.drawer-actions {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		margin-top: var(--sp-5);
	}

	/* ═════ Responsive ═════ */
	@media (max-width: 1200px) {
		.card-row { grid-template-columns: 1fr 1fr; grid-auto-rows: auto; gap: var(--sp-3); }
		.card-tier { grid-column: span 2; }
		.card-time { grid-column: span 2; align-items: flex-start; }
	}
	@media (max-width: 900px) {
		.stat-strip { grid-template-columns: repeat(3, 1fr); }
		.stat-tile:nth-child(4), .stat-tile:nth-child(5) { grid-column: auto; }
		.approval-card { flex-direction: column; align-items: stretch; }
		.card-actions { width: 100%; }
		.card-actions .btn { flex: 1; }
	}
	@media (max-width: 600px) {
		.stat-strip { grid-template-columns: repeat(2, 1fr); }
		.card-row { grid-template-columns: 1fr; }
		.card-tier, .card-time { grid-column: auto; }
		.drawer { width: 100vw; }
	}
</style>
