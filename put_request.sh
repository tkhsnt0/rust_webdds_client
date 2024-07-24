#!/bin/bash

# APIにPOSTリクエストする

### 引数一覧
# $1 : エンドポイント
# $2 : リクエストボディ(JSON)

### 変数定義

# エンドポイント
ENDPOINT="localhost:3000/sensor/config"

# リクエストボディ(JSON形式で受け取る)
JSON='{"sensor_type": "radio", "frequency":2100000, "power":300, "squelti":200}'

# リクエストを出す
curl -X PUT "${ENDPOINT}" \
    -H "Content-Type: application/json" \
    -d "${JSON}"
