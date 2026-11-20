module Main where

data Box a = Box a

bind :: forall a b. Box a -> (a -> Box b) -> Box b
bind (Box a) f = f a

identity :: forall a. a -> a
identity x = x

polyAndOuterRigid :: forall a. a -> a
polyAndOuterRigid value =
  let
    localIdentity = identity
    wrapped = wrap value
  in
    localIdentity wrapped
  where
  wrap x = x

class Tag a where
  tag :: a -> Box a

instance Tag (Box item) where
  tag value =
    let
      same = value
    in
      Box same

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class Named (s :: Symbol) where
  named :: Proxy s -> Int

instance Named s where
  named _ =
    let
      label = Proxy :: Proxy s
    in
      keepProxy label

keepProxy :: forall k (a :: k). Proxy a -> Int
keepProxy _ = 0

layoutAndCommentSkips :: Int
layoutAndCommentSkips =
  let singleLine = 1 in
    let
      -- leading line comment
      commentedLine = singleLine
      {- leading block comment -}
      commentedBlock = commentedLine
      clean = commentedBlock
    in
      finish clean
  where finish x = x

tabIndented :: Int
tabIndented =
  let
	tabbed = 1
  in
    tabbed

doSingleLine :: Box Int
doSingleLine = do
  let inlineValue = 1
  Box inlineValue
