module Main where

deeplyIndented :: Int -> Int
deeplyIndented seed =
  let
    outer =
      let
        longRecord =
          { firstVeryLongRecordFieldName: seed
          , secondVeryLongRecordFieldName: "hello"
          , thirdVeryLongRecordFieldName: true
          , fourthVeryLongRecordFieldName: seed
          }
        longFunction a b c d e f =
          pick a b c d e f
      in
        longFunction longRecord longRecord longRecord longRecord longRecord longRecord
  in
    outer
  where
  pick r _ _ _ _ _ = r.firstVeryLongRecordFieldName
