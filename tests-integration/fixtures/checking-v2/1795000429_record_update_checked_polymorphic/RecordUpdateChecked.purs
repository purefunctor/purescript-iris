module RecordUpdateChecked where

data Maybe a = Nothing | Just a

data Query a = Query a

type Spec q = { handle :: forall a. q a -> Maybe a, name :: String }

defaultSpec :: forall q. Spec q
defaultSpec = { handle: \_ -> Nothing, name: "" }

run :: forall q. Spec q -> String
run spec = spec.name

handle :: forall a. Query a -> Maybe a
handle (Query a) = Just a

checked :: String
checked = run (defaultSpec { handle = handle })

annotated :: Spec Query
annotated = defaultSpec { handle = handle, name = "annotated" }
