module Main where

import Int64Literal (int64Digits)

tenBillion :: String
tenBillion = int64Digits @10_000_000_000

maximum :: String
maximum = int64Digits @9223372036854775807

minimum :: String
minimum = int64Digits @(-9223372036854775808)
