FROM ubuntu:22.04

ENV CMD_BIN "/usr/local/bin/ajetod"

RUN apt-get -y update
RUN apt-get -y install ca-certificates
RUN apt-get -y install wget
RUN apt-get -y install libsqlite3-dev
RUN apt-get -y install sudo

RUN wget http://nz2.archive.ubuntu.com/ubuntu/pool/main/o/openssl/libssl1.1_1.1.1f-1ubuntu2.17_amd64.deb
RUN dpkg -i libssl1.1_1.1.1f-1ubuntu2.17_amd64.deb

COPY target/release/ajetod "${CMD_BIN}"

# Set the HOME environment variable
ENV HOME /home/ajeto

CMD ajetod
