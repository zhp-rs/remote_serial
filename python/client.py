#!/usr/bin/env python3
import socket
import argparse
import termios
import tty
import sys
import select
import signal

parser = argparse.ArgumentParser('remote serial tcp/ip client')
parser.add_argument('server', type=str, help='server ip add port, sample: 127.0.0.1:1234')
args = parser.parse_args()

orig_settings = termios.tcgetattr(sys.stdin)

def term_sig_handler(signum, frame):
    termios.tcsetattr(sys.stdin, termios.TCSADRAIN, orig_settings) 
    print()
    exit()

def main():
    signal.signal(signal.SIGTERM, term_sig_handler)
    try:
        client = socket.socket()
        ip, port = args.server.split(':')
        client.connect((ip, int(port)))
    except ConnectionRefusedError:
        print("connect error!") 
        return

    print("connected to {}:{}, Please Enter Control+X to exit!".format(*client.getpeername()))

    s_epoll = select.epoll()
    s_epoll.register(client.fileno(), select.POLLIN)
    s_epoll.register(sys.stdin.fileno(), select.POLLIN)

    tty.setraw(sys.stdin)

    while True:
        events = s_epoll.poll(2)
        for fileno, event in events:
          if fileno == client.fileno() and event == select.POLLIN:
                data = client.recv(1024).decode()
                print(data, end='', flush=True)
          elif fileno == sys.stdin.fileno() and event == select.POLLIN:
                data = sys.stdin.read(1).encode()
                if data == b'\x18': # Control+x
                    termios.tcsetattr(sys.stdin, termios.TCSADRAIN, orig_settings) 
                    print()
                    return
                client.send(data)

if __name__ == "__main__":
    main()
