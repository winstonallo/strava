```sh
echo <refresh token> > /srv/strava/refresh_token
docker run -d --restart unless-stopped -p 127.0.0.1:<host port>:3000 -v /srv/strava:/data -e STRAVA_CLIENT_ID=<client id> -e STRAVA_CLIENT_SECRET=<client secret> -e STRAVA_VERIFY_TOKEN=plsfortheloveofgodletmelogmyrideswithoutspammingmyfriends -e DOMAIN=<domain> ghcr.io/winstonallo/strava:latest
```
