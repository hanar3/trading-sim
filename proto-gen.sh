#!/bin/bash

echo "Generating protobuf files on api-gateway"
rm -rf ./api-gateway/src/generated  
mkdir ./api-gateway/src/generated
protoc --plugin=protoc-gen-ts=./api-gateway/node_modules/.bin/protoc-gen-ts_proto --ts_out=./api-gateway/src/generated --ts_opt=esModuleInterop=true,forceLong=long ./proto/trading.proto
echo "Done generating protobuf files on api-gateway"

