//! Compliance suite for the custody service.
//!
//! Integrates with external compliance providers for:
//! - AML/KYT transaction screening (Chainalysis, Elliptic, TRM Labs)
//! - OFAC/sanctions address screening
//! - Travel Rule compliance (Notabene, TRISA)
//!
//! Every withdrawal is screened before signing. Every deposit source
//! is screened after detection. High-risk transactions are flagged for review.

pub mod ofac;
pub mod ofac_downloader;
pub mod onchain_analysis;
pub mod risk;
pub mod screening;
pub mod travel_rule;

pub use screening::{ScreeningProvider, ScreeningResult, RiskLevel};
pub use ofac::{SanctionsList, SanctionsChecker};
pub use travel_rule::{TravelRuleProvider, TravelRuleData};
pub use risk::RiskAssessment;
