//! Functional expressions for the native React backend.

use crate::tree::ExpressionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactExpression {
    Component {
        render: ExpressionId,
    },
    Element {
        component: ReactElement,
        props: ExpressionId,
        key: Option<ExpressionId>,
        static_multiple_children: bool,
    },
    Fragment {
        children: ExpressionId,
    },
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactElement {
    Component(ExpressionId),
    Intrinsic(ExpressionId),
}

impl ReactExpression {
    pub fn try_for_each_child<E>(
        &self,
        mut visit: impl FnMut(ExpressionId) -> Result<(), E>,
    ) -> Result<(), E> {
        match self {
            ReactExpression::Component { render } => visit(*render)?,
            ReactExpression::Element { component, props, key, .. } => {
                match component {
                    ReactElement::Component(component) | ReactElement::Intrinsic(component) => {
                        visit(*component)?;
                    }
                }
                if let Some(key) = key {
                    visit(*key)?;
                }
                visit(*props)?;
            }
            ReactExpression::Fragment { children } => visit(*children)?,
            ReactExpression::Empty => {}
        }
        Ok(())
    }
}
