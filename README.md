## uchinopower

自宅のスマートメーター B ルートから消費電力量、瞬時電力、瞬時電流を得てデーターベースに蓄積する。

## 用意するもの

- ラズパイ
- BP35A1 とか RL7023 Stick-D/IPS とかの Sub-GHz WiSUN モジュール
- スマートメーター B ルートの接続情報

## 開発/実行環境

- ホスト側(開発環境) 64 ビット Ubuntu (Windows 11 上の WSL)
- ターゲット側は 64 ビット Raspberry OS (Raspberry Pi 3)

## コンパイル＆ビルド

```
cargo build --release
```

## コンパイル＆ビルド＆実行

```
cargo run --bin pairing -- --help
```

```
cargo run --bin dryrun -- --help
```

など

## クロスコンパイル(ターゲット側は Raspberry Pi 3)

ラズパイでビルドするのは非常に遅いので、クロスコンパイルする。

### クロスコンパイル環境の整備(開発環境)

arch64 の musl 版のツールチェーンをインストールする。

```
rustup target add aarch64-unknown-linux-musl
```

### クロスビルド

```
cargo build --release --target aarch64-unknown-linux-musl
```

### 実行ファイルをラズパイに送る

- target/aarch64-unknown-linux-musl/release/dryrun
- target/aarch64-unknown-linux-musl/release/pairing
- target/aarch64-unknown-linux-musl/release/uchino_daqd

target/ディレクトリにある このバイナリが生成物。
このファイルをラズパイに転送。

### 実行権限を付与

ラズパイのコンソールで

```
chmod +x dryrun pairing uchino_daqd
```

## スマートメーター B ルート接続をデーターベース無しで実行する

WiSUN モジュールをラズパイの USB に挿す。
または BP35A1 をシリアル接続する。

```
ls /dev
```

すると USB 接続なら **/dev/ttyUSB0** などがある。

WiSUN モジュールのデバイスは --device で指定する。

### スマートメーターをアクティブスキャンで探す。(pairing)

```
./dryrun pairing --id "BルートID" --password "Bルートパスワード"
```

### スマートメーター B ルートから情報を得る。(dryrun)

```
$ ./dryrun dry-run
```

```
[2025-07-13T01:32:33Z INFO  dryrun] Get_resプロパティ値読み出し応答 N=2 瞬時電力= 1068 W 瞬時電流:(1φ3W) R= 9.8 A, T= 2.2 A
```

こんなかんじで瞬時電力が出力される。
これ以後設定ファイル(uchinopower.toml)は不要なので消去する。

## PostgreSQL データーベースを準備する

### ラズパイに postgresql をインストールする。

ラズパイのコンソールで

```
sudo apt update

sudo apt install postgres
```

### postgres ユーザーのパスワードを設定する

```
sudo passwd postgres
```

### postgres ユーザー(ロール)作成

```
pi@raspberrypi:/home/pi$ sudo su postgres -

postgres@raspberrypi:/home/pi$ createuser -P postgres
Enter password for new role:
```

好きなパスワードを入力する。 例えば raspberry とか

### ラズパイの IP アドレスを確認する

```
pi@raspberrypi:~ $ hostname
raspberrypi

pi@raspberrypi:~ $ ip addr
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
       valid_lft forever preferred_lft forever
    inet6 ::1/128 scope host
       valid_lft forever preferred_lft forever
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc pfifo_fast state UP group default qlen 1000
    link/ether b8:27:eb:e4:1d:45 brd ff:ff:ff:ff:ff:ff
    inet 192.168.1.6/24 brd 192.168.1.255 scope global dynamic noprefixroute eth0
       valid_lft 8930sec preferred_lft 7130sec
    inet6 2400:4153:8082:af00:247e:6450:5fdb:d7ee/64 scope global dynamic mngtmpaddr noprefixroute
       valid_lft 13601sec preferred_lft 11801sec
    inet6 fe80::1cc1:af13:5613:d821/64 scope link
       valid_lft forever preferred_lft forever
3: wlan0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc pfifo_fast state DOWN group default qlen 1000
    link/ether b8:27:eb:b1:48:10 brd ff:ff:ff:ff:ff:ff
pi@raspberrypi:~ $
```

このラズパイは有線接続なので、インターフェースは eth0 を見る。

- ホスト名: raspberrypi
- IP アドレス: 192.168.1.6/24

Windows の WSL から確認する。

```
$ ip addr
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
       valid_lft forever preferred_lft forever
    inet6 ::1/128 scope host
       valid_lft forever preferred_lft forever
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP group default qlen 1000
    link/ether 04:0e:3c:18:df:ef brd ff:ff:ff:ff:ff:ff
    inet 192.168.1.5/24 brd 192.168.1.255 scope global noprefixroute eth0
       valid_lft forever preferred_lft forever
    inet6 2400:4153:8082:af00:a1cb:5bad:bae2:728c/64 scope global nodad deprecated noprefixroute
       valid_lft forever preferred_lft 0sec
    inet6 2400:4153:8082:af00:418a:ef97:4ce2:7d85/128 scope global nodad noprefixroute
       valid_lft forever preferred_lft forever
    inet6 fe80::495e:6fc2:809f:ef0f/64 scope link nodad noprefixroute
       valid_lft forever preferred_lft forever
3: loopback0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP group default qlen 1000
    link/ether 00:15:5d:ed:c1:20 brd ff:ff:ff:ff:ff:ff
4: eth1: <BROADCAST,MULTICAST> mtu 1500 qdisc mq state DOWN group default qlen 1000
    link/ether 40:5b:d8:0f:62:25 brd ff:ff:ff:ff:ff:ff
5: docker0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN group default
    link/ether be:cb:dc:20:4d:37 brd ff:ff:ff:ff:ff:ff
    inet 172.17.0.1/16 brd 172.17.255.255 scope global docker0
       valid_lft forever preferred_lft forever
```

WSL 側の IP アドレス: 192.168.1.5/24

```
$ ping raspberrypi.local
PING raspberrypi.local (192.168.1.6) 56(84) bytes of data.
64 bytes from 192.168.1.6: icmp_seq=1 ttl=64 time=1.02 ms
64 bytes from 192.168.1.6: icmp_seq=2 ttl=64 time=0.794 ms
64 bytes from 192.168.1.6: icmp_seq=3 ttl=64 time=0.793 ms
64 bytes from 192.168.1.6: icmp_seq=4 ttl=64 time=0.784 ms
64 bytes from 192.168.1.6: icmp_seq=5 ttl=64 time=0.841 ms
^C
--- raspberrypi.local ping statistics ---
5 packets transmitted, 5 received, 0% packet loss, time 4089ms
rtt min/avg/max/mdev = 0.784/0.845/1.017/0.087 ms
```

WSL からラズパイに ping で疎通確認できた。

### pg_hba.conf を編集

/etc/postgresql/ バージョン番号 /main/pg_hba.conf を編集して、
192.168.1.1/24 のネットワークからパスワード認証でアクセスできるようにする。

```
# IPv4 local connections:
host    all             all             127.0.0.1/32            md5
```

に 192.168.1.1/24 の行を追加する。

```
# IPv4 local connections:
host    all             all             127.0.0.1/32            md5
host    all             all             192.168.1.1/24          md5
```

### postgresql.conf を編集

/etc/postgresql/ バージョン番号 /main/postgresql.conf を編集する。

```
# - Connection Settings -

listen_addresses = '*'          # what IP address(es) to listen on;
```

listen_addresses を '\*' にする。

### postgresql を起動する

```
sudo service postgresql start
```

ラズパイのポート 5432 (PostgreSQL)が LISTEN であることを確認する

```
pi@raspberrypi:~ $ ss -pluten | grep 5432
tcp   LISTEN 0      244          0.0.0.0:5432       0.0.0.0:*    uid:115 ino:259899 sk:1 cgroup:/system.slice/system-postgresql.slice/postgresql@13-main.service <->
tcp   LISTEN 0      244             [::]:5432          [::]:*    uid:115 ino:259900 sk:4 cgroup:/system.slice/system-postgresql.slice/postgresql@13-main.service v6only:1 <->
```

## SQLx cli をインストール

ホスト側 WSL で

```
cargo install sqlx-cli
```

## .env ファイルを作成

- サーバー: raspberrypi.local
- ポート: 5432
- ユーザー: posgres
- パスワード: raspberry
- データーベース名: uchinopower

なので

```
DATABASE_URL=postgres://postgres:raspberry@raspberrypi.local:5432/uchinopower
```

と ".env" ファイルに書いて保存する。

### データーベースの作成

```
sqlx database create
```

### migration の実行

```
$ sqlx migrate run
Applied 20250712120721/migrate uchinopower (230.414189ms)
```

### データーベースを確認する

ラズパイのコンソールで

```
pi@raspberrypi:~ $ sudo -u postgres psql
psql (13.21 (Debian 13.21-0+deb11u1))
Type "help" for help.

postgres=#
```

`\l` と入力。

```
pi@raspberrypi:~ $ sudo -u postgres psql
psql (13.21 (Debian 13.21-0+deb11u1))
Type "help" for help.

postgres=# \l
                               List of databases
    Name     |  Owner   | Encoding | Collate |  Ctype  |   Access privileges
-------------+----------+----------+---------+---------+-----------------------
 postgres    | postgres | UTF8     | C.UTF-8 | C.UTF-8 |
 template0   | postgres | UTF8     | C.UTF-8 | C.UTF-8 | =c/postgres          +
             |          |          |         |         | postgres=CTc/postgres
 template1   | postgres | UTF8     | C.UTF-8 | C.UTF-8 | =c/postgres          +
             |          |          |         |         | postgres=CTc/postgres
 uchinopower | postgres | UTF8     | C.UTF-8 | C.UTF-8 |
(4 rows)

postgres=#

```

uchinopower データーベースがある。

`\c uchinopower` と入力。

```
postgres=# \c uchinopower
You are now connected to database "uchinopower" as user "postgres".
uchinopower=#
```

`\d` と入力。

```
uchinopower=# \d
                       List of relations
 Schema |              Name              |   Type   |  Owner
--------+--------------------------------+----------+----------
 public | _sqlx_migrations               | table    | postgres
 public | cumlative_amount_epower        | table    | postgres
 public | cumlative_amount_epower_id_seq | sequence | postgres
 public | instant_current                | table    | postgres
 public | instant_current_id_seq         | sequence | postgres
 public | instant_epower                 | table    | postgres
 public | instant_epower_id_seq          | sequence | postgres
 public | settings                       | table    | postgres
 public | settings_id_seq                | sequence | postgres
(9 rows)

uchinopower=#
```

テーブルが確認できた。
`Ctrl-D`を入力して終了。

## linux サービスとして動かす

常駐プログラム(service, daemon)として動かす。

実行ファイルを /usr/local/sbin/ にコピーする。
ラズパイのコンソールで

```
sudo cp uchino_daqd /usr/local/sbin/
```

### スマートメーターをアクティブスキャンで探す。(pairing)

スマートメーターをアクティブスキャンで探して接続情報をデーターベースに入れる。  
自身がデーターベースサーバーなのでここでは DATABASE_URL が localhost になる。

```
$ export DATABASE_URL=postgres://postgres:raspberry@localhost:5432/uchinopower
$ export SERIAL_DEVICE=/dev/ttyUSB0
$ ./pairing "BルートID" "Bルートパスワード
```

**スマートメーターを探しているので、しばらく待つ...**

事前に `export RUST_LOG=trace` しておくと 何をしているかが出力される。

```
pi@raspberrypi:~ $ ./pairing "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" "xxxxxxxxxxxx"
[2025-07-13T04:36:32Z INFO  uchinoepower::pairing] Get_resプロパティ値読み出し応答 N=1 積算電力量単位(正方向、逆方向計測値)= 0.01 kwh
[2025-07-13T04:36:37Z INFO  uchinoepower::pairing] INFプロパティ値通知 N=1 インスタンスリスト= 1個 [028801]
[2025-07-13T04:36:42Z INFO  uchinoepower::pairing] Get_resプロパティ値読み出し応答 N=1 Getプロパティマップ [0x80,0x81,0x82,0x88,0x8A,0x8D,0x97,0x98,0x9D,0x9E,0x9F,0xD7,0xE0,0xE1,0xE2,0xE5,0xE7,0xE8,0xEA]
[2025-07-13T04:36:47Z INFO  uchinoepower::pairing] Get_SNAプロパティ値読み出し不可応答 N=1 係数=1
successfully finished, id=1
```

### systemctl サービスファイルを作る

```
sudo nvim /etc/systemd/system/uchinopower.service
```

自身がデーターベースサーバーなのでここでも DATABASE_URL が localhost になる。

```
[Unit]
Description=data acquisition from smartmeter route-B
After=syslog.target network.target postgresql.service

[Service]
Type=forking
PIDFile=/run/uchino_daqd.pid
ExecStart=/usr/local/sbin/uchino_daqd
WorkingDirectory=/tmp
KillMode=process
Restart=always
Environment=SERIAL_DEVICE=/dev/ttyUSB0
Environment=DATABASE_URL=postgres://postgres:raspberry@localhost:5432/uchinopower

[Install]
WantedBy=multi-user.target
```

### 起動

```
pi@raspberrypi:~ $ sudo systemctl start uchinopower.service
```

### 確認

```
pi@raspberrypi:~ $ systemctl status uchinopower.service
● uchinopower.service - data acquisition from smartmeter route-B
     Loaded: loaded (/etc/systemd/system/uchinopower.service; enabled; vendor preset: enabled)
     Active: active (running) since Sun 2025-07-13 15:08:30 JST; 15s ago
   Main PID: 1905 (uchino_daqd)
      Tasks: 1 (limit: 779)
        CPU: 36ms
     CGroup: /system.slice/uchinopower.service
             └─1905 /usr/local/sbin/uchino_daqd

Jul 13 15:08:30 raspberrypi systemd[1]: Starting data acquisition from smartmeter route-B...
Jul 13 15:08:30 raspberrypi systemd[1]: Started data acquisition from smartmeter route-B.
```

#### 自動起動有効化

正常起動を確認してから自動起動設定をする。

```
pi@raspberrypi:~ $ sudo systemctl enable uchinopower.service
Created symlink /etc/systemd/system/multi-user.target.wants/uchinopower.service → /etc/systemd/system/uchinopower.service.
```

### 自動起動の確認(しなくてもいい)

```
sudo reboot
```

### データーの確認

```
pi@raspberrypi:~ $ sudo -u postgres psql
psql (13.21 (Debian 13.21-0+deb11u1))
Type "help" for help.

postgres=# \c uchinopower
You are now connected to database "uchinopower" as user "postgres".
uchinopower=# select * from cumlative_amount_epower ;
 id | location |      recorded_at       |   kwh
----+----------+------------------------+----------
  1 |          | 2025-07-13 15:30:00+09 | 18799.57
  2 |          | 2025-07-13 16:00:00+09 | 18800.31
  3 |          | 2025-07-13 16:30:00+09 | 18801.04
  4 |          | 2025-07-13 17:00:00+09 | 18801.79
(4 rows)

uchinopower=# select * from instant_epower ;
id | location |      recorded_at       | watt
----+----------+------------------------+------
  1 |          | 2025-07-13 15:41:00+09 | 1432
  2 |          | 2025-07-13 15:42:00+09 | 1384
  3 |          | 2025-07-13 15:43:00+09 | 1424
  4 |          | 2025-07-13 15:44:00+09 | 1224
  5 |          | 2025-07-13 15:45:00+09 | 1756
  6 |          | 2025-07-13 15:47:00+09 | 1808
  7 |          | 2025-07-13 15:48:00+09 | 1868
  8 |          | 2025-07-13 15:49:00+09 | 1668
  9 |          | 2025-07-13 15:50:00+09 | 1612
 10 |          | 2025-07-13 15:51:00+09 | 1628
 11 |          | 2025-07-13 15:52:00+09 | 1764
 12 |          | 2025-07-13 15:53:00+09 | 1680
 13 |          | 2025-07-13 15:54:00+09 | 1612
 14 |          | 2025-07-13 15:55:00+09 | 1624
 15 |          | 2025-07-13 15:56:00+09 | 1600
 16 |          | 2025-07-13 15:57:00+09 | 1640
 17 |          | 2025-07-13 15:58:00+09 | 1620
 18 |          | 2025-07-13 15:59:00+09 | 1612
 19 |          | 2025-07-13 16:00:00+09 | 1616
 20 |          | 2025-07-13 16:01:00+09 | 1620
 21 |          | 2025-07-13 16:02:00+09 | 1652
 22 |          | 2025-07-13 16:03:00+09 | 1620
 23 |          | 2025-07-13 16:04:00+09 | 1580
 24 |          | 2025-07-13 16:05:00+09 | 1632
 25 |          | 2025-07-13 16:06:00+09 | 1636
 26 |          | 2025-07-13 16:07:00+09 | 1620
 27 |          | 2025-07-13 16:08:00+09 | 1584
 28 |          | 2025-07-13 16:09:00+09 | 1616
 29 |          | 2025-07-13 16:10:00+09 | 1532
 30 |          | 2025-07-13 16:11:00+09 | 1592
 31 |          | 2025-07-13 16:12:00+09 | 1576
 32 |          | 2025-07-13 16:13:00+09 | 1456
 33 |          | 2025-07-13 16:14:00+09 | 1460
 34 |          | 2025-07-13 16:15:00+09 | 1512
 35 |          | 2025-07-13 16:16:00+09 | 1324
 36 |          | 2025-07-13 16:17:00+09 | 1392
 37 |          | 2025-07-13 16:18:00+09 | 1424
 38 |          | 2025-07-13 16:19:00+09 | 1340
 39 |          | 2025-07-13 16:20:00+09 | 1324
 40 |          | 2025-07-13 16:21:00+09 | 1324
 41 |          | 2025-07-13 16:22:00+09 | 1320
 42 |          | 2025-07-13 16:23:00+09 | 1320
 43 |          | 2025-07-13 16:24:00+09 | 1328
 44 |          | 2025-07-13 16:25:00+09 | 1312
 45 |          | 2025-07-13 16:26:00+09 | 1312
 46 |          | 2025-07-13 16:27:00+09 | 1372
 47 |          | 2025-07-13 16:28:00+09 | 1392
 48 |          | 2025-07-13 16:29:00+09 | 1308
 49 |          | 2025-07-13 16:30:00+09 | 1456
 50 |          | 2025-07-13 16:31:00+09 | 1416
 51 |          | 2025-07-13 16:32:00+09 | 1432
 52 |          | 2025-07-13 16:33:00+09 | 1424
 53 |          | 2025-07-13 16:34:00+09 | 1512
 54 |          | 2025-07-13 16:35:00+09 | 1456
 55 |          | 2025-07-13 16:36:00+09 | 1452
 56 |          | 2025-07-13 16:37:00+09 | 1444
 57 |          | 2025-07-13 16:38:00+09 | 1476
 58 |          | 2025-07-13 16:39:00+09 | 1668
 59 |          | 2025-07-13 16:40:00+09 | 1512
 60 |          | 2025-07-13 16:41:00+09 | 1504
 61 |          | 2025-07-13 16:42:00+09 | 1488
 62 |          | 2025-07-13 16:43:00+09 | 1476
 63 |          | 2025-07-13 16:44:00+09 | 1480
 64 |          | 2025-07-13 16:45:00+09 | 1484
 65 |          | 2025-07-13 16:46:00+09 | 1476
 66 |          | 2025-07-13 16:47:00+09 | 1488
 67 |          | 2025-07-13 16:48:00+09 | 1488
 68 |          | 2025-07-13 16:49:00+09 | 1468
 69 |          | 2025-07-13 16:50:00+09 | 1540
 70 |          | 2025-07-13 16:51:00+09 | 1552
 71 |          | 2025-07-13 16:52:00+09 | 1524
 72 |          | 2025-07-13 16:53:00+09 | 1512
 73 |          | 2025-07-13 16:54:00+09 | 1520
 74 |          | 2025-07-13 16:55:00+09 | 1516
 75 |          | 2025-07-13 16:56:00+09 | 1508
 76 |          | 2025-07-13 16:57:00+09 | 1508
 77 |          | 2025-07-13 16:58:00+09 | 1520
 78 |          | 2025-07-13 16:59:00+09 | 1516
 79 |          | 2025-07-13 17:00:00+09 | 1512
 80 |          | 2025-07-13 17:01:00+09 | 1504
 81 |          | 2025-07-13 17:02:00+09 | 1508
 82 |          | 2025-07-13 17:03:00+09 | 1512
 83 |          | 2025-07-13 17:04:00+09 | 1508
 84 |          | 2025-07-13 17:05:00+09 | 1496
 85 |          | 2025-07-13 17:06:00+09 | 1508
 86 |          | 2025-07-13 17:07:00+09 | 1508
 87 |          | 2025-07-13 17:08:00+09 | 1512
 88 |          | 2025-07-13 17:09:00+09 | 1752
 89 |          | 2025-07-13 17:10:00+09 | 1752
 90 |          | 2025-07-13 17:11:00+09 | 1508
 91 |          | 2025-07-13 17:12:00+09 | 1496
 92 |          | 2025-07-13 17:13:00+09 | 1500
 93 |          | 2025-07-13 17:14:00+09 | 1444
 94 |          | 2025-07-13 17:15:00+09 | 1436
 95 |          | 2025-07-13 17:16:00+09 | 1436
 96 |          | 2025-07-13 17:17:00+09 | 1716
 97 |          | 2025-07-13 17:19:00+09 | 2144
 98 |          | 2025-07-13 17:20:00+09 | 2172
(98 rows)

uchinopower=# select * from instant_current ;
 id | location |      recorded_at       |  r   |  t
----+----------+------------------------+------+-----
  1 |          | 2025-07-13 15:41:00+09 | 13.5 | 2.0
  2 |          | 2025-07-13 15:42:00+09 | 13.5 | 1.5
  3 |          | 2025-07-13 15:43:00+09 | 13.4 | 2.1
  4 |          | 2025-07-13 15:44:00+09 | 10.6 | 2.5
  5 |          | 2025-07-13 15:45:00+09 | 16.9 | 2.1
  6 |          | 2025-07-13 15:47:00+09 | 17.7 | 2.1
  7 |          | 2025-07-13 15:48:00+09 | 17.9 | 2.3
  8 |          | 2025-07-13 15:49:00+09 | 16.1 | 2.1
  9 |          | 2025-07-13 15:50:00+09 | 15.8 | 2.0
 10 |          | 2025-07-13 15:51:00+09 | 15.8 | 2.0
 11 |          | 2025-07-13 15:52:00+09 | 16.7 | 2.5
 12 |          | 2025-07-13 15:53:00+09 | 15.9 | 2.6
 13 |          | 2025-07-13 15:54:00+09 | 15.8 | 2.0
 14 |          | 2025-07-13 15:55:00+09 | 15.8 | 2.3
 15 |          | 2025-07-13 15:56:00+09 | 15.6 | 2.2
 16 |          | 2025-07-13 15:57:00+09 | 15.9 | 2.3
 17 |          | 2025-07-13 15:58:00+09 | 15.7 | 2.3
 18 |          | 2025-07-13 15:59:00+09 | 15.7 | 2.2
 19 |          | 2025-07-13 16:00:00+09 | 15.8 | 2.2
 20 |          | 2025-07-13 16:01:00+09 | 15.7 | 2.2
 21 |          | 2025-07-13 16:02:00+09 | 16.0 | 2.2
 22 |          | 2025-07-13 16:03:00+09 | 15.7 | 2.2
 23 |          | 2025-07-13 16:04:00+09 | 15.4 | 2.2
 24 |          | 2025-07-13 16:05:00+09 | 15.3 | 2.5
 25 |          | 2025-07-13 16:06:00+09 | 15.5 | 2.4
 26 |          | 2025-07-13 16:07:00+09 | 15.1 | 2.7
 27 |          | 2025-07-13 16:08:00+09 | 15.4 | 2.1
 28 |          | 2025-07-13 16:09:00+09 | 15.2 | 2.5
 29 |          | 2025-07-13 16:10:00+09 | 14.9 | 2.0
 30 |          | 2025-07-13 16:11:00+09 | 15.0 | 2.4
 31 |          | 2025-07-13 16:12:00+09 | 14.9 | 2.5
 32 |          | 2025-07-13 16:13:00+09 | 14.3 | 2.0
 33 |          | 2025-07-13 16:14:00+09 | 14.3 | 1.9
 34 |          | 2025-07-13 16:15:00+09 | 14.3 | 2.4
 35 |          | 2025-07-13 16:16:00+09 | 12.7 | 1.9
 36 |          | 2025-07-13 16:17:00+09 | 12.8 | 2.5
 37 |          | 2025-07-13 16:18:00+09 | 12.7 | 2.8
 38 |          | 2025-07-13 16:19:00+09 | 13.0 | 1.9
 39 |          | 2025-07-13 16:20:00+09 | 12.8 | 1.8
 40 |          | 2025-07-13 16:21:00+09 | 12.8 | 1.8
 41 |          | 2025-07-13 16:22:00+09 | 12.8 | 1.8
 42 |          | 2025-07-13 16:23:00+09 | 12.8 | 1.8
 43 |          | 2025-07-13 16:24:00+09 | 12.8 | 1.9
 44 |          | 2025-07-13 16:25:00+09 | 12.7 | 1.9
 45 |          | 2025-07-13 16:26:00+09 | 12.7 | 1.9
 46 |          | 2025-07-13 16:27:00+09 | 12.6 | 2.4
 47 |          | 2025-07-13 16:28:00+09 | 12.6 | 2.6
 48 |          | 2025-07-13 16:29:00+09 | 12.6 | 1.9
 49 |          | 2025-07-13 16:30:00+09 | 14.5 | 1.8
 50 |          | 2025-07-13 16:31:00+09 | 13.9 | 1.9
 51 |          | 2025-07-13 16:32:00+09 | 14.1 | 1.9
 52 |          | 2025-07-13 16:33:00+09 | 14.0 | 1.9
 53 |          | 2025-07-13 16:34:00+09 | 13.9 | 2.8
 54 |          | 2025-07-13 16:35:00+09 | 14.1 | 2.1
 55 |          | 2025-07-13 16:36:00+09 | 14.2 | 2.1
 56 |          | 2025-07-13 16:37:00+09 | 14.1 | 2.1
 57 |          | 2025-07-13 16:38:00+09 | 14.2 | 2.3
 58 |          | 2025-07-13 16:39:00+09 | 14.3 | 4.0
 59 |          | 2025-07-13 16:40:00+09 | 14.8 | 2.1
 60 |          | 2025-07-13 16:41:00+09 | 14.7 | 2.1
 61 |          | 2025-07-13 16:42:00+09 | 14.5 | 2.1
 62 |          | 2025-07-13 16:43:00+09 | 14.4 | 2.1
 63 |          | 2025-07-13 16:44:00+09 | 14.5 | 2.1
 64 |          | 2025-07-13 16:45:00+09 | 14.5 | 2.1
 65 |          | 2025-07-13 16:46:00+09 | 14.4 | 2.1
 66 |          | 2025-07-13 16:47:00+09 | 14.6 | 2.1
 67 |          | 2025-07-13 16:48:00+09 | 14.5 | 2.3
 68 |          | 2025-07-13 16:49:00+09 | 14.4 | 2.1
 69 |          | 2025-07-13 16:50:00+09 | 15.1 | 2.1
 70 |          | 2025-07-13 16:51:00+09 | 15.2 | 2.1
 71 |          | 2025-07-13 16:52:00+09 | 14.9 | 2.1
 72 |          | 2025-07-13 16:53:00+09 | 14.8 | 2.1
 73 |          | 2025-07-13 16:54:00+09 | 14.9 | 2.2
 74 |          | 2025-07-13 16:55:00+09 | 14.9 | 2.2
 75 |          | 2025-07-13 16:56:00+09 | 14.8 | 2.2
 76 |          | 2025-07-13 16:57:00+09 | 14.8 | 2.1
 77 |          | 2025-07-13 16:58:00+09 | 14.9 | 2.1
 78 |          | 2025-07-13 16:59:00+09 | 14.7 | 2.2
 79 |          | 2025-07-13 17:00:00+09 | 14.9 | 2.1
 80 |          | 2025-07-13 17:01:00+09 | 14.7 | 2.1
 81 |          | 2025-07-13 17:02:00+09 | 14.7 | 2.1
 82 |          | 2025-07-13 17:03:00+09 | 14.8 | 2.2
 83 |          | 2025-07-13 17:04:00+09 | 14.7 | 2.1
 84 |          | 2025-07-13 17:05:00+09 | 14.7 | 2.1
 85 |          | 2025-07-13 17:06:00+09 | 14.8 | 2.1
 86 |          | 2025-07-13 17:07:00+09 | 14.7 | 2.1
 87 |          | 2025-07-13 17:08:00+09 | 14.6 | 2.2
 88 |          | 2025-07-13 17:09:00+09 | 16.9 | 2.1
 89 |          | 2025-07-13 17:10:00+09 | 16.9 | 2.1
 90 |          | 2025-07-13 17:11:00+09 | 14.7 | 2.1
 91 |          | 2025-07-13 17:12:00+09 | 14.7 | 2.1
 92 |          | 2025-07-13 17:13:00+09 | 14.6 | 2.1
 93 |          | 2025-07-13 17:14:00+09 | 14.1 | 2.1
 94 |          | 2025-07-13 17:15:00+09 | 14.0 | 2.1
 95 |          | 2025-07-13 17:16:00+09 | 13.9 | 2.1
 96 |          | 2025-07-13 17:17:00+09 | 16.7 | 2.3
 97 |          | 2025-07-13 17:19:00+09 | 21.5 | 2.2
 98 |          | 2025-07-13 17:20:00+09 | 21.3 | 2.5
 99 |          | 2025-07-13 17:21:00+09 | 21.9 | 2.1
(99 rows)

uchinopower=#
```

このように 1 分毎にデーターをデータベースに蓄積する。
