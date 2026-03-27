<script lang="ts">
	// Policy management UI — placeholder until /v1/policies is implemented.
	// Rules defined here map to ABAC policies enforced by the signing service.

	const TIERS = [
		{ range: '< $1K + whitelisted', approval: 'Auto',              delay: '1–5 min' },
		{ range: '$1K – $10K',          approval: 'Auto + alert',       delay: '10 min' },
		{ range: '$10K – $100K',        approval: '1 approver (FIDO2)', delay: '30 min' },
		{ range: '$100K – $500K',       approval: '2 approvers',        delay: '2 hours' },
		{ range: '> $500K',             approval: '3 approvers, cold',  delay: '24 hours' },
	];
</script>

<h1>Policy Rules</h1>

<div class="card">
	<h3>Withdrawal Approval Tiers</h3>
	<table>
		<thead>
			<tr><th>Tier</th><th>Amount</th><th>Approval Required</th><th>Delay</th></tr>
		</thead>
		<tbody>
			{#each TIERS as tier, i}
				<tr>
					<td class="tier">{i}</td>
					<td>{tier.range}</td>
					<td>{tier.approval}</td>
					<td class="muted">{tier.delay}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>

<div class="card">
	<h3>Circuit Breaker Limits</h3>
	<div class="limits">
		<div class="limit-row"><span>Per-transaction cap</span><span class="val">$500,000</span></div>
		<div class="limit-row"><span>Hourly outflow limit</span><span class="val">$10,000,000</span></div>
		<div class="limit-row"><span>Daily outflow limit</span><span class="val">$50,000,000</span></div>
	</div>
	<p class="muted note">Configurable via <code>config/default.toml</code>. Changes require restart.</p>
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	h3 { margin: 0 0 0.9rem; font-size: 0.95rem; color: var(--fg-muted, #8b949e); }

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.tier { font-weight: bold; color: var(--accent, #58a6ff); width: 3rem; }
	.muted { color: var(--fg-muted, #8b949e); }

	.limits { display: flex; flex-direction: column; gap: 0.5rem; }

	.limit-row {
		display: flex;
		justify-content: space-between;
		padding: 0.4rem 0;
		border-bottom: 1px solid var(--border, #30363d);
		font-size: 0.9rem;
	}

	.val { font-family: monospace; color: var(--yellow, #d29922); }

	.note { margin: 0.8rem 0 0; font-size: 0.8rem; }

	code {
		background: var(--hover-bg, #1c2128);
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
		font-size: 0.8rem;
	}
</style>
