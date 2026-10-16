-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

class Base value <= Derived value
builtinBind = do
  value <- action
  pure value
existingUnicode ∷ ∀ value. value → value
existingUnicode value = value
labels :: forall value. { forall :: value } -> value
labels record = record.forall
symbols = "forall :: <- -> <= => ∀ ∷ ← → ⇐ ⇒"
-- forall :: <- -> <= => ∀ ∷ ← → ⇐ ⇒
customOperators = first <= second ⊕ third <=> fourth
type CustomOperator = First <= Second
