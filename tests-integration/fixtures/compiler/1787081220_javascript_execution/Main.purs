module Main where

import Library (Wrapped(..), forward, wrapped)
import Effect (Effect)

foreign import addInt :: Int -> Int -> Int
foreign import decrementInt :: Int -> Int
foreign import equalInt :: Int -> Int -> Boolean
foreign import foreignValue :: Int
foreign import effectValue :: Effect Int
foreign import await :: Int

integer :: Int
integer = 42

number :: Number
number = 1.5

string :: String
string = "iris"

array :: Array Int
array = [1, 2, 3]

type Model =
  { count :: Int
  , nested :: { enabled :: Boolean }
  , "hostile-field" :: Int
  , "__proto__" :: String
  }

model :: Model
model =
  { count: 0
  , nested: { enabled: true }
  , "hostile-field": await
  , "__proto__": "data, not a prototype"
  }

updated :: Model
updated = model { count = 1, nested { enabled = false } }

readHostile :: Model -> Int
readHostile value = value."hostile-field"

readProto :: Model -> String
readProto value = value."__proto__"

capture :: Int -> Int -> Int
capture captured = \_ -> captured

apply :: forall a b. (a -> b) -> a -> b
apply function value = function value

addCaptured :: Int -> Int -> Int
addCaptured amount = \value -> addInt amount value

curried :: Int
curried = apply (addCaptured 2) 40

nestedJoin :: Boolean -> Boolean -> Int
nestedJoin outer inner =
  if outer then
    let
      captured = foreignValue
      result = case inner of
        true -> 1
        false -> 2
    in addInt captured result
  else 0

countdown :: Int -> Int
countdown value =
  if equalInt value 0 then 0
  else addInt 1 (countdown (decrementInt value))

isEven :: Int -> Boolean
isEven value =
  if equalInt value 0 then true
  else isOdd (decrementInt value)

isOdd :: Int -> Boolean
isOdd value =
  if equalInt value 0 then false
  else isEven (decrementInt value)

capturedMutual :: Int -> Boolean -> Int
capturedMutual captured condition = localFirst condition
  where
  localFirst :: Boolean -> Int
  localFirst true = captured
  localFirst false = localSecond true

  localSecond :: Boolean -> Int
  localSecond true = captured
  localSecond false = localFirst true

data Choice = None | Pair Int Int

pair :: Choice
pair = Pair 7 8

first :: Choice -> Int
first choice = case choice of
  None -> 0
  Pair left _ -> left

partialPattern :: Choice -> Int
partialPattern (Pair left _) = left

unwrapWrapped :: Wrapped -> Int
unwrapWrapped (Wrapped value) = value

crossModule :: Int
crossModule = unwrapWrapped wrapped

forwardReference :: Int
forwardReference = forward

class Measure a where
  measure :: a -> Int

instance Measure Int where
  measure = addInt 1

evidenceValue :: Int
evidenceValue = measure 41
