﻿all: Program.cs
	mkdir -p build/
	nuget restore SteamAuth/SteamAuth/SteamAuth.sln
	mcs -target:library -out:build/SteamAuth.dll -r:SteamAuth/SteamAuth/packages/Newtonsoft.Json.7.0.1/lib/net45/Newtonsoft.Json.dll SteamAuth/SteamAuth/APIEndpoints.cs SteamAuth/SteamAuth/AuthenticatorLinker.cs SteamAuth/SteamAuth/Confirmation.cs SteamAuth/SteamAuth/SessionData.cs SteamAuth/SteamAuth/SteamGuardAccount.cs SteamAuth/SteamAuth/SteamWeb.cs SteamAuth/SteamAuth/TimeAligner.cs SteamAuth/SteamAuth/UserLogin.cs SteamAuth/SteamAuth/Util.cs SteamAuth/SteamAuth/Properties/AssemblyInfo.cs
	cp SteamAuth/SteamAuth/packages/Newtonsoft.Json.7.0.1/lib/net45/Newtonsoft.Json.dll build/
	mcs -out:build/steamguard -r:build/SteamAuth.dll -r:build/Newtonsoft.Json.dll -r:/usr/lib/mono/4.5/System.Security.dll Program.cs Manifest.cs

run:
	build/steamguard -v

clean:
	rm -r build/