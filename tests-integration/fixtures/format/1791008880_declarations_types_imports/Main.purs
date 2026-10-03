module Main(Thing(..),longFunction)where
import Data.Array as Array
import Data.Maybe(Maybe(..))
data Thing a=Thing a|Empty
type Pair a={first::a,second::a}
newtype Wrapped a=Wrapped(Array a)
longFunction::forall a.(a->a)->Maybe a->Maybe a
longFunction function value=case value of
 Just item->Just(function item)
 Nothing->Nothing
