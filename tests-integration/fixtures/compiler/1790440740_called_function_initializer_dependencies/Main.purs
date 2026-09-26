module Main where

data Parser = Parser (Int -> Int)

bindFlipped :: (Int -> Parser) -> Parser -> Parser
bindFlipped callback (Parser parser) = Parser \state ->
  case callback (parser state) of
    Parser next -> next state

fail :: Int -> Parser
fail message = bindFlipped (\_ -> Parser \_ -> message) position

plusParser = { empty: fail 7 }

position :: Parser
position = Parser \state -> state

test :: Int
test = case plusParser.empty of
  Parser parser -> parser 42

conditional :: Int
conditional = choose false

choose :: Boolean -> Int
choose condition = if condition then conditional else 19
