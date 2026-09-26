module Main where

import Prelude

import Data.HeytingAlgebra as HeytingAlgebra
import Data.Ring as Ring
import Data.Semiring as Semiring
import Lookalike as Lookalike

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

booleanNot :: Boolean -> Boolean
booleanNot value = HeytingAlgebra.not value

booleanConj :: Boolean -> Boolean -> Boolean
booleanConj left right = HeytingAlgebra.conj left right

booleanDisj :: Boolean -> Boolean -> Boolean
booleanDisj left right = HeytingAlgebra.disj left right

partiallyAppliedConj :: Boolean -> Boolean
partiallyAppliedConj = HeytingAlgebra.conj true

genericConj :: forall value. HeytingAlgebra.HeytingAlgebra value => value -> value -> value
genericConj left right = HeytingAlgebra.conj left right

conjOrder :: Boolean -> Boolean -> Boolean
conjOrder left right = observe "left" left && observe "right" right

disjOrder :: Boolean -> Boolean -> Boolean
disjOrder left right = observe "left" left || observe "right" right

conjCase :: Boolean -> Boolean -> Boolean
conjCase left right = observe "left" left && case observe "right" right of
  true -> true
  false -> false

disjCase :: Boolean -> Boolean -> Boolean
disjCase left right = observe "left" left || case observe "right" right of
  true -> true
  false -> false

conjFunctionOrder :: Boolean -> Boolean -> Boolean
conjFunctionOrder left right =
  observe "function" (\value -> value) (observe "left" left && case observe "right" right of
    true -> true
    false -> false)

conjPatternFunctionOrder :: Boolean -> Boolean -> Boolean
conjPatternFunctionOrder left right =
  observe "function" (\value -> value)
    (observe "left" left && (\{ value } -> value) { value: observe "right" right })

integerAdd :: Int -> Int -> Int
integerAdd left right = Semiring.add left right

inlineIntegerAdd :: Int -> Int -> Int
inlineIntegerAdd left right =
  let result = Semiring.add left right
  in result

integerSubtract :: Int -> Int -> Int
integerSubtract left right = Ring.sub left right

integerMultiply :: Int -> Int -> Int
integerMultiply left right = Semiring.mul left right

integerNegate :: Int -> Int
integerNegate value = Ring.negate value

integerNegateLiteral :: Int
integerNegateLiteral = Ring.negate 20

inlineIntegerNegateLiteral :: Int
inlineIntegerNegateLiteral =
  let value = 20
  in Ring.negate value

numberNegate :: Number -> Number
numberNegate value = Ring.negate value

numberNegateLiteral :: Number
numberNegateLiteral = Ring.negate 20.5

numberNegateZero :: Number
numberNegateZero = Ring.negate 0.0

genericNegate :: forall value. Ring.Ring value => value -> value
genericNegate value = Ring.negate value

partiallyAppliedNegate :: Int -> Int
partiallyAppliedNegate = Ring.negate

integerAddOrder :: Boolean -> Int
integerAddOrder _ =
  Semiring.add
    (observe "left" 20)
    (observe "right" 22)

partiallyAppliedAdd :: Int -> Int
partiallyAppliedAdd = Semiring.add 1

lookalikeAdd :: Int -> Int -> Int
lookalikeAdd left right = Lookalike.add left right

lookalikeNegate :: Int -> Int
lookalikeNegate value = Lookalike.negate value
