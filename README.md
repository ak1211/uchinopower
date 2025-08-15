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

### screen コマンドで試してみる(しなくてもいい)。

```
pi@raspberrypi:~ $ screen /dev/ttyUSB0 115200
```

SKINFO と入力してエンター

```
SKINFO
EINFO FE80:0000:0000:0000:xxxx:xxxx:xxxx:xxxx xxxxxxxxxxxxxxxx xx xxxx xxxx
OK
```

### screen コマンドの終了

C-a k (Ctrl+a を押した後に k)
そして y を入力。

**/dev/ttyUSB0** が WiSUN モジュールだと確認できた。
pi ユーザー(このユーザー)に権限がない場合は, 以下のように dialout グループに pi ユーザーを追加する。

```
pi@raspberrypi:~ $ ls -l /dev/ttyUSB0
crw-rw---- 1 root dialout 188, 0 Aug 11 10:39 /dev/ttyUSB0
```

dialout グループに読み書き権限がある。

```
pi@raspberrypi:~ $ grep dialout /etc/group
dialout:x:20:
```

dialout グループに pi ユーザーを入れる。

```
pi@raspberrypi:~ $ sudo usermod -aG dialout pi
pi@raspberrypi:~ $ grep dialout /etc/group
dialout:x:20:pi
```

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

sudo apt install postgresql
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
- IPv4 アドレス: 192.168.1.6/24
- IPv6 アドレス: 2400:4153:8082:af00:247e:6450:5fdb:d7ee/64

### WSL の IP アドレスを確認する

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

WSL 側の イーサネットアドレス

- IPv4 アドレス: 192.168.1.5/24
- IPv6 アドレス: 2400:4153:8082:af00:a1cb:5bad:bae2:728c/64

### WSL のコンソールからラズパイとの疎通確認

WSL のコンソールから ラズパイに向けて ping を打つ。

```
$ ping 192.168.1.6
PING 192.168.1.6 (192.168.1.6) 56(84) bytes of data.
64 bytes from 192.168.1.6: icmp_seq=1 ttl=64 time=0.825 ms
64 bytes from 192.168.1.6: icmp_seq=2 ttl=64 time=2.73 ms
64 bytes from 192.168.1.6: icmp_seq=3 ttl=64 time=0.773 ms
^C
--- 192.168.1.6 ping statistics ---
3 packets transmitted, 3 received, 0% packet loss, time 2189ms
rtt min/avg/max/mdev = 0.773/1.441/2.725/0.908 ms
```

```
aki@DESKTOP-JAGH6CN:~$ ping 2400:4153:8082:af00:247e:6450:5fdb:d7ee
PING 2400:4153:8082:af00:247e:6450:5fdb:d7ee (2400:4153:8082:af00:247e:6450:5fdb:d7ee) 56 data bytes
64 bytes from 2400:4153:8082:af00:247e:6450:5fdb:d7ee: icmp_seq=1 ttl=64 time=1.47 ms
64 bytes from 2400:4153:8082:af00:247e:6450:5fdb:d7ee: icmp_seq=2 ttl=64 time=0.725 ms
64 bytes from 2400:4153:8082:af00:247e:6450:5fdb:d7ee: icmp_seq=3 ttl=64 time=0.621 ms
64 bytes from 2400:4153:8082:af00:247e:6450:5fdb:d7ee: icmp_seq=4 ttl=64 time=0.667 ms
64 bytes from 2400:4153:8082:af00:247e:6450:5fdb:d7ee: icmp_seq=5 ttl=64 time=0.777 ms
^C
--- 2400:4153:8082:af00:247e:6450:5fdb:d7ee ping statistics ---
5 packets transmitted, 5 received, 0% packet loss, time 4448ms
rtt min/avg/max/mdev = 0.621/0.851/1.465/0.311 ms
```

### 'raspberrypi.local'との疎通確認

mDNS によって'raspberrypi.local'の名前解決がなされているかを確認しておく。

WSL コンソールで

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

ついでに PowerShell で

```
PS C:\> ping raspberrypi.local

raspberrypi.local [2400:4153:8082:af00:247e:6450:5fdb:d7ee]に ping を送信しています 32 バイトのデータ:
2400:4153:8082:af00:247e:6450:5fdb:d7ee からの応答: 時間 <1ms
2400:4153:8082:af00:247e:6450:5fdb:d7ee からの応答: 時間 <1ms
2400:4153:8082:af00:247e:6450:5fdb:d7ee からの応答: 時間 <1ms
2400:4153:8082:af00:247e:6450:5fdb:d7ee からの応答: 時間 <1ms

2400:4153:8082:af00:247e:6450:5fdb:d7ee の ping 統計:
    パケット数: 送信 = 4、受信 = 4、損失 = 0 (0% の損失)、
ラウンド トリップの概算時間 (ミリ秒):
    最小 = 0ms、最大 = 0ms、平均 = 0ms
```

WSL からラズパイに ping で疎通確認できた。

### pg_hba.conf を編集

/etc/postgresql/ バージョン番号 /main/pg_hba.conf を編集して、
192.168.1.1/24 と 2400:4153:8082:af00::1 /64 のネットワークからパスワード認証でアクセスできるようにする。

```
# IPv4 local connections:
host    all             all             127.0.0.1/32            md5
# IPv6 local connections:
host    all             all             ::1/128                 md5
```

に 192.168.1.1/24 と 2400:4153:8082:af00::1/64 の行を追加する。

```
# IPv4 local connections:
host    all             all             127.0.0.1/32            md5
host    all             all             192.168.1.1/24          md5
# IPv6 local connections:
host    all             all             ::1/128                 md5
host    all             all             2400:4153:8082:af00::1/64  md5
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

postgresq.service 稼働中に設定ファイルを編集した場合は、このコマンドで `sudo systemctl reload postgresql.service` 再読み込みする。

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
After=syslog.target network.target

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

お好みで

```
Environment=RUST_LOG=trace
```

などを加えてもいいですね。

### service ファイルを再読み込みする

```
pi@raspberrypi:~ $ sudo systemctl daemon-reload
```

### ログ書込みディレクトリを作る

```
pi@raspberrypi:~ $ sudo install -m 775 -o daemon -g daemon -d /var/log/uchinopower/
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

正しく動作していると 1 分毎にデータベースに蓄積する。

```
pi@raspberrypi:~ $ sudo -u postgres psql
psql (13.21 (Debian 13.21-0+deb11u1))
Type "help" for help.

postgres=# \c uchinopower
You are now connected to database "uchinopower" as user "postgres".
uchinopower=# select now();
              now
-------------------------------
 2025-08-15 10:05:02.081141+09
(1 row)

uchinopower=# select * from cumlative_amount_epower order by recorded_at desc limit 10;
  id  | location |      recorded_at       |   kwh
------+----------+------------------------+----------
 4062 |          | 2025-08-15 10:00:00+09 | 19489.00
 4061 |          | 2025-08-15 09:30:00+09 | 19488.52
 4060 |          | 2025-08-15 09:00:00+09 | 19488.01
 4059 |          | 2025-08-15 08:30:00+09 | 19487.42
 4058 |          | 2025-08-15 08:00:00+09 | 19486.90
 4057 |          | 2025-08-15 07:30:00+09 | 19486.37
 4056 |          | 2025-08-15 07:00:00+09 | 19485.79
 4055 |          | 2025-08-15 06:30:00+09 | 19485.27
 4054 |          | 2025-08-15 06:00:00+09 | 19484.87
 4053 |          | 2025-08-15 05:30:00+09 | 19484.57
(10 rows)

uchinopower=# select * from instant_epower order by recorded_at desc limit 10;
  id   | location |      recorded_at       | watt
-------+----------+------------------------+------
 39828 |          | 2025-08-15 10:05:00+09 |  944
 39827 |          | 2025-08-15 10:04:00+09 |  988
 39826 |          | 2025-08-15 10:03:00+09 |  936
 39825 |          | 2025-08-15 10:02:00+09 |  828
 39824 |          | 2025-08-15 10:01:00+09 |  784
 39823 |          | 2025-08-15 10:00:00+09 |  796
 39822 |          | 2025-08-15 09:59:00+09 |  808
 39821 |          | 2025-08-15 09:58:00+09 |  812
 39820 |          | 2025-08-15 09:57:00+09 |  840
 39819 |          | 2025-08-15 09:56:00+09 |  740
(10 rows)

uchinopower=# select * from instant_current order by recorded_at desc limit 10;
  id   | location |      recorded_at       |  r  |  t
-------+----------+------------------------+-----+-----
 39828 |          | 2025-08-15 10:05:00+09 | 8.4 | 1.7
 39827 |          | 2025-08-15 10:04:00+09 | 8.4 | 2.1
 39826 |          | 2025-08-15 10:03:00+09 | 8.4 | 1.7
 39825 |          | 2025-08-15 10:02:00+09 | 7.0 | 1.7
 39824 |          | 2025-08-15 10:01:00+09 | 6.6 | 1.7
 39823 |          | 2025-08-15 10:00:00+09 | 6.7 | 1.7
 39822 |          | 2025-08-15 09:59:00+09 | 6.8 | 1.7
 39821 |          | 2025-08-15 09:58:00+09 | 6.8 | 1.7
 39820 |          | 2025-08-15 09:57:00+09 | 7.2 | 1.7
 39819 |          | 2025-08-15 09:56:00+09 | 6.2 | 1.7
(10 rows)

uchinopower=#
```

## LibreOffice Base で確認する

### Base データーベースウイザード

Base データーベースウイザードを開いて

1. 既存のデーターベースに接続を選択して「PostgreSQL」を選ぶ  
   ![](https://github.com/user-attachments/assets/b28e8c8b-06bf-4e87-95d5-a0eaeb25ca62)
2. データーベース名「uchonopower」、サーバー「raspberrypi.local」を入力  
   ![](https://github.com/user-attachments/assets/d6382f6c-e8bb-4615-a275-f7500c6169fa)
3. ユーザー名「postgres」、パスワードを要求するにチェックを入れて「接続のテスト」をクリック  
   ![](https://github.com/user-attachments/assets/95ed7924-5ebf-4bcf-a940-73e07d619987)
4. パスワードは「raspberry」  
   ![](https://github.com/user-attachments/assets/ff8807e5-f803-420e-a341-413bb28ea3c2)
5. テスト結果成功を確認する  
   ![](https://github.com/user-attachments/assets/7299090a-b973-4b8a-92fe-3e8cfb783b98)

完了したら保存する。

### Base クエリ

SQL 表示でクエリーを作成からクエリーデザインを開いて

```
select * from instant_epower order by recorded_at desc;
```

を入力して「クエリーの実行」で確認する

![](https://github.com/user-attachments/assets/538bec74-46bf-4565-b58a-4e09b42b2d14)

確認出来たら正順にするために desc を消して

```
select * from instant_epower order by recorded_at desc;
```

これを「クエリー 1」で保存する。

「クエリー 1」をダブルクリックして実行する。

![](https://github.com/user-attachments/assets/e5ddc495-9de5-43f3-b602-336bce6759e2)

あとは Calc のデーターソースに、この Base データーベースを使って、グラフ化すればよいですね。
