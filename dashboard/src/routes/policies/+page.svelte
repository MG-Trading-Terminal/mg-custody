<script lang="ts">
	import { api } from '$lib/api';
	import { onMount } from 'svelte';

	interface PolicyRule {
		id: string;
		name: string;
		priority: number;
		enabled: boolean;
		conditions: string;
		action: string;
	}

	interface CircuitBreakerStatus {
		is_tripped: boolean;
		single_tx_limit_usd: string;
		hourly_limit_usd: string;
		daily_limit_usd: string;
		hourly_outflow_usd: string;
		daily_outflow_usd: string;
		trip_reason?: string;
	}

	let rules = $state<PolicyRule[]>([]);
	let cbStatus = $state<CircuitBreakerStatus | null>(null);
	let loading = $state(true);
	let error = $state('');

	const TIERS = [
		{ tier: 0, range: '< $1K + whitelisted', approval: 'Auto',              delay: '1-5 min' },
		{ tier: 1, range: '$1K - $10K',           approval: 'Auto + alert',       delay: '10 min' },
		{ tier: 2, range: '$10K - $100K',         approval: '1 approver (FIDO2)', delay: '30 min' },
		{ tier: 3, range: '$100K - $500K',        approval: '2 approvers',        delay: '2 hours' },
		{ tier: 4, range: '> $500K',              approval: '3 approvers + cold',  delay: '24 hours' },
	];

	async function loadData() {
		loading = true;
		error = '';

		// Load policy rules (best-effort)
		try {
			rules = await api.getJson<PolicyRule[]>('/v1/admin/policies');
		} catch {
			// API may not exist yet — use empty
			rules = [];
		}

		// Load circuit breaker status
		try {
			cbStatus = await api.getJson<CircuitBreakerStatus>('/v1/circuit-breaker/status');
		} catch {
			cbStatus = null;
		}

		loading = false;
	}

	function fmtUsd(val: string | undefined): string {
		if (!val) return '-';
		const n = parseFloat(val);
		if (isNaN(n)) return val;
		return '$' + n.toLocaleString('en-US', { maximumFractionDigits: 0 });
	}

	function pctOfLimit(outflow: string | undefined, limit: string | undefined): number {
		if (!outflow || !limit) return 0;
		const o = parseFloat(outflow);
		const l = parseFloat(limit);
		if (l <= 0) return 0;
		return Math.min(100, (o / l) * 100);
	}

	onMount(loadData);
</script>

<h1>Policy Rules</h1>

{#if error}
	<div class="card err">{error}</div>
{/if}

<!-- Circuit Breaker Status (live) -->
<div class="card">
	<h3>Circuit Breaker
		{#if cbStatus}
			{#if cbStatus.is_tripped}
				<span class="status-badge tripped">TRIPPED</span>
			{:else}
				<span class="status-badge normal">Normal</span>
			{/if}
		{/if}
	</h3>

	{#if cbStatus}
		{#if cbStatus.is_tripped}
			<div class="alert-box">Reason: {cbStatus.trip_reason ?? 'Unknown'}</div>
		{/if}

		<div class="limits">
			<div class="limit-row">
				<span>Per-transaction cap</span>
				<span class="val">{fmtUsd(cbStatus.single_tx_limit_usd)}</span>
			</div>
			<div class="limit-row">
				<span>Hourly outflow</span>
				<div class="bar-group">
					<div class="bar-bg">
						<div class="bar-fill" style="width: {pctOfLimit(cbStatus.hourly_outflow_usd, cbStatus.hourly_limit_usd)}%"></div>
					</div>
					<span class="val">{fmtUsd(cbStatus.hourly_outflow_usd)} / {fmtUsd(cbStatus.hourly_limit_usd)}</span>
				</div>
			</div>
			<div class="limit-row">
				<span>Daily outflow</span>
				<div class="bar-group">
					<div class="bar-bg">
						<div class="bar-fill" style="width: {pctOfLimit(cbStatus.daily_outflow_usd, cbStatus.daily_limit_usd)}%"></div>
					</div>
					<span class="val">{fmtUsd(cbStatus.daily_outflow_usd)} / {fmtUsd(cbStatus.daily_limit_usd)}</span>
				</div>
			</div>
		</div>
	{:else}
		<p class="muted">Unable to load circuit breaker status.</p>
	{/if}
</div>

<!-- Withdrawal Tiers -->
<div class="card">
	<h3>Withdrawal Approval Tiers</h3>
	<div class="table-wrap"><table>
		<thead>
			<tr><th>Tier</th><th>Amount</th><th>Approval Required</th><th>Delay</th></tr>
		</thead>
		<tbody>
			{#each TIERS as t}
				<tr>
					<td class="tier">{t.tier}</td>
					<td>{t.range}</td>
					<td>{t.approval}</td>
					<td class="muted">{t.delay}</td>
				</tr>
			{/each}
		</tbody>
	</table></div>
</div>

<!-- Custom Policy Rules (from API) -->
{#if rules.length > 0}
	<div class="card">
		<h3>Custom Rules ({rules.length})</h3>
		<div class="table-wrap"><table>
			<thead>
				<tr><th>#</th><th>Name</th><th>Conditions</th><th>Action</th><th>Enabled</th></tr>
			</thead>
			<tbody>
				{#each rules as rule}
					<tr class:disabled={!rule.enabled}>
						<td class="tier">{rule.priority}</td>
						<td>{rule.name}</td>
						<td class="muted mono">{rule.conditions}</td>
						<td><span class="action-badge">{rule.action}</span></td>
						<td>{rule.enabled ? 'Yes' : 'No'}</td>
					</tr>
				{/each}
			</tbody>
		</table></div>
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	.err { color: var(--red); border-color: rgba(248, 81, 73, 0.3); background: rgba(248, 81, 73, 0.1); }

	h3 {
		margin: 0 0 0.9rem;
		font-size: 0.95rem;
		color: var(--fg-muted);
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}

	.status-badge {
		font-size: 0.7rem;
		font-weight: 700;
		text-transform: uppercase;
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
	}

	.status-badge.normal { background: rgba(63, 185, 80, 0.15); color: var(--green); }
	.status-badge.tripped { background: rgba(248, 81, 73, 0.15); color: var(--red); animation: pulse 1s infinite; }

	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.6; } }

	.alert-box {
		background: rgba(248, 81, 73, 0.1);
		border: 1px solid rgba(248, 81, 73, 0.3);
		border-radius: 6px;
		padding: 0.6rem 0.8rem;
		margin-bottom: 0.8rem;
		color: var(--red);
		font-size: 0.88rem;
	}

	.limits { display: flex; flex-direction: column; gap: 0.6rem; }

	.limit-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.4rem 0;
		border-bottom: 1px solid var(--border);
		font-size: 0.9rem;
	}

	.bar-group { display: flex; align-items: center; gap: 0.6rem; flex: 1; max-width: 300px; }

	.bar-bg {
		flex: 1;
		height: 6px;
		background: var(--hover-bg);
		border-radius: 3px;
		overflow: hidden;
	}

	.bar-fill {
		height: 100%;
		background: var(--accent);
		border-radius: 3px;
		transition: width 0.3s;
	}

	.val { font-family: monospace; color: var(--yellow, #d29922); font-size: 0.85rem; white-space: nowrap; }

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted); border-bottom: 1px solid var(--border); font-size: 0.8rem; text-transform: uppercase; }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border); }

	.tier { font-weight: bold; color: var(--accent); width: 3rem; }
	.muted { color: var(--fg-muted); font-size: 0.85rem; }
	.mono { font-family: monospace; font-size: 0.8rem; }

	.action-badge {
		background: var(--hover-bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.1rem 0.4rem;
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: uppercase;
	}

	tr.disabled { opacity: 0.5; }
</style>
