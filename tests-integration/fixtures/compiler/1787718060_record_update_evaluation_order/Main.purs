module Main where

type Inner =
  { value :: Int
  , untouched :: Int
  }

type Nested =
  { value :: Int
  , inner :: Inner
  , untouched :: Int
  }

type Model =
  { first :: Int
  , nested :: Nested
  , last :: Int
  , untouched :: Int
  }

foreign import failAt :: String -> Boolean -> Int -> Int

foreign import makeModel :: String -> Model

foreign import observe :: String -> Int -> Int

updateLocal :: Model -> Model
updateLocal source =
  source
    { first = observe "local-first" 10
    , nested { value = observe "local-nested" 20 }
    , last = observe "local-last" 30
    }

updateCall :: Boolean -> Model
updateCall shouldThrow =
  (makeModel "call")
    { first = observe "call-first" 10
    , nested { value = failAt "call-nested" shouldThrow 20 }
    , last = observe "call-last" 30
    }

updateDeep :: Boolean -> Model
updateDeep _ =
  (makeModel "deep")
    { nested
        { value = observe "deep-nested" 50
        , inner { value = observe "deep-inner" 60 }
        }
    }

updateControl :: Boolean -> Model -> Model
updateControl condition source =
  source
    { first = observe "control-first" 10
    , nested
        { value =
            case condition of
              true -> observe "control-then" 30
              false -> observe "control-else" 31
        }
    , last = observe "control-last" 32
    }

updateOpen
  :: forall row
   . { first :: Int | row }
  -> { first :: Int | row }
updateOpen source = source { first = observe "open-first" 40 }
