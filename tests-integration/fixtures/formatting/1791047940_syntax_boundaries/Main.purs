-- @format width=40 incomplete=false
-- @format width=80
-- @format width=40 indent=4 unicode=true
module Main where

data Void
data Box (a :: Type) = Box a
newtype Wrapped a = Wrapped a
type role Wrapped representational
type EmptyRow = ()
type EmptyRecord = {}
type OpenRow r = ( | r )
type OpenRecord r = { | r }
type RowFields r = ( field :: Int, other :: String | r )
type RecordFields r = { field :: Int, other :: String | r }
type Quantified = forall @a (b :: Type). a -> b
type Kinded = Int :: Type
type Signed = P -1
type Arrow = Int -> Int
type Constrained = Show a => a
type Operator = Int + Int

class C a
class D a b | -> a, a -> b
instance C Int
derive instance C
class Eq a <= Ordered a where
  compare :: a -> a -> Int
instance Show a => Show (Wrapped a) where
  show :: Wrapped a -> String
  show (Wrapped value) = show value

foreign import external :: Int
foreign import data External :: Type
data Kind :: Type
newtype NewKind :: Type
type AliasKind :: Type
class ClassKind :: Type -> Constraint

value :: Int
value = 1
local = let value :: Int
            value = 1
        in value
patternLocal = let Tuple a b = pair in a
guarded value | Just result <- lookup value, result > 0 = result
conditional = if condition then 1 else 2
caseResult = case value of
  Just result -> result
multipleCase = case first, second of
  Just result, Nothing -> result
lambda = \value -> value
parenthesized = (1)
negative = -1
typed = (1 :: Int)
typeArgument = identity @Int 1
operator = (1 + 2)
operatorName = (+)
section = _ + 1
infixValue = a `f` b
tickChain = a `f + g` b
record = { a: 1 }
recordPun = { value }
recordUpdate = record { a = 2, nested { field = 3 } }
empty = { record: {}, array: [] }
emptyAdo = ado in 1
hole = ?hole

binders {} [] named@value (item :: Int) { field: field } = value
binderOperator (firstElement : remainingElements) = firstElement
whereValue = result
  where
  result = 1
doValue = do
  value <- action
  value
doEndingInBind = do
  value <- action
doEndingInLet = do
  let value = 1
adoValue = ado
  value <- action
  in value
