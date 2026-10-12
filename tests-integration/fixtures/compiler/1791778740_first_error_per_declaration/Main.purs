module Main where

independentMismatches :: { first :: Int, second :: Int }
independentMismatches = { first: "first", second: true }

signatureAndEquation :: Missing
signatureAndEquation = missing

warningWithoutError :: Boolean -> Int
warningWithoutError true = 1

warningAfterError :: Boolean -> Int
warningAfterError true = "one"
