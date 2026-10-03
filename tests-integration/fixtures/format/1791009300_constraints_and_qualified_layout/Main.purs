module Main where

import Control.Monad as Monad
class(Eq a,Show a)<=Convert a b|a->b where
 convert::a->b
instance first::Convert Int String where
 convert value="first"
else instance fallback::Convert a String where
 convert _="fallback"
derive newtype instance another::Eq Wrapper
foreign import data Wrapper::Type
type role Wrapper representational
test::forall @a (row::Row Type).a->{where::a|row}->a
test value record=record.where
updates record=record{where=value,nested{other=1}}
negateTwice value= - -value
apply=identity @Int 1
backticks left right=left`append`right
testDo=Monad.do
  let first=1
      second=2
  pure(first+second)
testAdo=Monad.ado
  first<-pure 1
  second<-pure 2
  in {first,second}
