#!/bin/sh
mkdir -p data/
cd data/

HOST=localhost
IP="127.0.0.1"
CERTIFICATE_CN="/C=CZ/O=Ivanovo/CN=$HOST"
VALIDITY=3650
KEY_FILE=$HOST.key
CERT_FILE=$HOST.crt
REQ_FILE=$HOST.csr
CA_FILE=ca.pem


# Create unencrypted private key and a CSR (certificate signing request)
openssl req -newkey rsa:2048 -nodes -subj "$CERTIFICATE_CN" -keyout $KEY_FILE -out $REQ_FILE


# Create a self-signed root CA
openssl x509 -signkey $KEY_FILE -in $REQ_FILE -req -days $VALIDITY -out $CA_FILE
#openssl req -x509 -sha256 -nodes -subj "$CERTIFICATE_CN" -days $VALIDITY -newkey rsa:2048 -keyout rootCA.key -out rootCA.crt


# Create file localhost.ext with the following content:
cat <<EOF > .ca.ext
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
subjectAltName = @alt_names
[alt_names]
DNS.1 = $HOST
IP.1 = $IP
EOF

# Sign the CSR (`cert.pem`) with the root CA certificate and private key
# => this overwrites `cert.pem` because it gets signed
openssl x509 -req -CA $CA_FILE -CAkey $KEY_FILE -in $REQ_FILE -out $CERT_FILE -days $VALIDITY -CAcreateserial -extfile .ca.ext