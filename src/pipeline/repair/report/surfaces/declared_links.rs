use crate::pipeline::repair::{declared_links::check_declared_links, RepairFinding, RepairSurface};

use super::{RepairContext, SurfaceCheck};

pub struct DeclaredLinksCheck;

impl SurfaceCheck for DeclaredLinksCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::DeclaredLinks
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        vec![check_declared_links(ctx.synrepo_dir)]
    }
}
