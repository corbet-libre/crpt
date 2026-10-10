use crate::{
    cost::{self, DispatchPermit, Mode},
    payments::*,
};

pub fn permit(request: &Instruction, mode: Mode) -> DispatchPermit<String> {
    let amount = cost::Amount {
        unit: cost::Unit {
            code: "fixture-fee".into(),
            scale: 0,
        },
        atoms: 0,
    };
    cost::Plan::new(
        vec![cost::Offer {
            candidate: cost::Candidate::new(request.adapter.clone(), [request.operation.clone()]),
            quote: cost::Quote {
                price: cost::Price::Exact(amount.clone()),
                source: "inert fixture".into(),
                valid_until: u64::MAX,
                binding: cost::binding(request, mode),
            },
        }],
        cost::Config::new([request.adapter.clone()], [request.operation.clone()]),
        cost::CostOrder::Configured,
        amount,
        1,
        mode,
        0,
    )
    .unwrap()
    .start()
    .begin(0)
    .unwrap()
}
pub trait FixturePayments {
    async fn execute(&self, request: &Instruction, now: u64) -> Result<Execution>;
}
impl<J: Journal> FixturePayments for Payments<J> {
    async fn execute(&self, request: &Instruction, now: u64) -> Result<Execution> {
        self.dispatch(request, permit(request, Mode::Production), now)
            .await
    }
}
