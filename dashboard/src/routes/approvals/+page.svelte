<script lang="ts">
	// Approval API is not yet wired. This page will be populated when
	// the /v1/approvals endpoint is ready. See ROADMAP.md Phase 17.
</script>

<h1>Pending Approvals</h1>

<div class="card empty">
	<div class="icon">A</div>
	<p>No pending approvals.</p>
	<p class="muted">
		Withdrawals requiring human sign-off will appear here.
		Tier 2+ withdrawals require FIDO2 authentication.
	</p>
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 200px;
		gap: 0.5rem;
		text-align: center;
	}

	.icon {
		width: 3rem;
		height: 3rem;
		border-radius: 50%;
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 1.3rem;
		font-weight: bold;
		color: var(--accent, #58a6ff);
		margin-bottom: 0.5rem;
	}

	p { margin: 0.25rem 0; font-size: 0.95rem; }
	.muted { color: var(--fg-muted, #8b949e); font-size: 0.85rem; max-width: 360px; }
</style>
