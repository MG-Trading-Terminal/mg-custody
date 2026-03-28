<script lang="ts">
	// User management UI — placeholder until /v1/users is implemented.

	const ROLES = [
		{ name: 'Viewer',  desc: 'Read-only: balances, transactions, addresses.' },
		{ name: 'Editor',  desc: 'Can initiate withdrawals and generate addresses.' },
		{ name: 'Signer',  desc: 'Can approve Tier 2–3 withdrawal requests (FIDO2).' },
		{ name: 'Admin',   desc: 'Full access except key export and role promotion.' },
		{ name: 'Owner',   desc: 'Unrestricted. Can promote to Admin. Requires MFA.' },
	];
</script>

<h1>User Management</h1>

<div class="card empty">
	<p>User management will be available in a future release.</p>
	<p class="muted">Invite links and FIDO2 enrollment will appear here.</p>
</div>

<div class="card">
	<h3>Role Legend</h3>
	<div class="table-wrap"><table>
		<thead>
			<tr><th>Role</th><th>Permissions</th></tr>
		</thead>
		<tbody>
			{#each ROLES as r}
				<tr>
					<td><span class="badge">{r.name}</span></td>
					<td class="muted">{r.desc}</td>
				</tr>
			{/each}
		</tbody>
	</table></div>
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

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		padding: 2rem 1.2rem;
		gap: 0.4rem;
	}

	h3 { margin: 0 0 0.9rem; font-size: 0.95rem; color: var(--fg-muted, #8b949e); }

	.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); vertical-align: top; }

	.badge {
		display: inline-block;
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 4px;
		padding: 0.15rem 0.5rem;
		font-size: 0.8rem;
		color: var(--accent, #58a6ff);
		white-space: nowrap;
	}

	p { margin: 0.2rem 0; font-size: 0.95rem; }
	.muted { color: var(--fg-muted, #8b949e); font-size: 0.85rem; }
</style>
