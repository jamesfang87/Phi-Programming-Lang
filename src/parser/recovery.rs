use chumsky::Parser as ChumskyParser;
use chumsky::input::InputRef;
use chumsky::prelude::*;

use crate::driver::source::SrcSpan;
use crate::lexer::token::{ITEM_STARTERS, STATEMENT_STARTERS, Token, TokenKind};

use super::Extra;

pub(crate) struct SkipRules {
    stop_before: &'static [TokenKind],
    stop_after: &'static [TokenKind],
}

pub(crate) const ITEM_RECOVERY: SkipRules = SkipRules {
    stop_before: ITEM_STARTERS,
    stop_after: &[],
};

pub(crate) const STATEMENT_RECOVERY: SkipRules = SkipRules {
    stop_before: STATEMENT_STARTERS,
    stop_after: &[TokenKind::Semicolon],
};

pub(crate) fn recover_by_skipping<'a, O: 'a>(
    rules: SkipRules,
    build: impl Fn(SrcSpan) -> O + Clone + 'a,
) -> impl ChumskyParser<'a, &'a [Token], O, Extra<'a>> + Clone + 'a {
    custom(move |inp: &mut InputRef<'a, '_, &'a [Token], Extra<'a>>| {
        let first: Token = inp.parse(any())?;

        let mut skipped = first.span;
        let mut depth = usize::from(first.kind.opens_group());

        while let Some(token) = inp.peek() {
            if depth == 0 {
                if rules.stop_before.contains(&token.kind) {
                    break;
                }
                if rules.stop_after.contains(&token.kind) {
                    inp.skip();
                    skipped = skipped.merge(token.span);
                    break;
                }
                if token.kind.closes_group() {
                    break;
                }
            }

            if token.kind.opens_group() {
                depth += 1;
            } else if token.kind.closes_group() {
                depth -= 1;
            }
            inp.skip();
            skipped = skipped.merge(token.span);
        }

        Ok(build(skipped))
    })
}
