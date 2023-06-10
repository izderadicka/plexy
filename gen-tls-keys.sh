#!/bin/sh
mkdir -p data/
cd data/

HOST=localhost
IP="127.0.0.1"
CERTIFICATE_CN="/C=CZ/O=Ivanovo/CN=$HOST"
VALIDITY=3650

# source: https://users.rust-lang.org/t/use-tokio-tungstenite-with-rustls-instead-of-native-tls-for-secure-websockets/90130

# Create unencrypted private key and a CSR (certificate signing request)
openssl req -newkey rsa:2048 -nodes -subj "$CERTIFICATE_CN" -keyout key.pem -out key.csr


# Create a self-signed root CA
openssl req -x509 -sha256 -nodes -subj "$CERTIFICATE_CN" -days $VALIDITY -newkey rsa:2048 -keyout rootCA.key -out rootCA.crt


# Create file localhost.ext with the following content:
cat <<EOF > localhost.ext
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
subjectAltName = @alt_names
[alt_names]
DNS.1 = $HOST
IP.1 = $IP
EOF

# Sign the CSR (`cert.pem`) with the root CA certificate and private key
# => this overwrites `cert.pem` because it gets signed
openssl x509 -req -CA rootCA.crt -CAkey rootCA.key -in key.csr -out cert.pem -days $VALIDITY -CAcreateserial -extfile localhost.ext