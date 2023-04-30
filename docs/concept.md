# Version control



# Introduction

## Starting situation

## Goals
Browser game like "Travian" as a "MMO" but in style of Age of Empries 2.


# Game principles

## Browser game
The game shall run solely in a web browser, meaning that all standard web browser should be supported - namely Google Chrome, Mozilla Firefox, Microsoft Edge.

## Game / World setup
The game consists of multiple playable worlds in which every user can create an account an start playing. Each world is basically an server instance of "Eperica" which is the name for a randomly generated map.

### Playmodes
Each world has a specific playmode it is made for when generated randomly. This allows players to choose what type of play style they prefer the most.

#### Many vs Many
Many vs. Many is the default play mode where up to 10.000 players can join a single world and play together. There is an end goal the players need to achieve to complete the world.

#### 1 vs 1
1 vs. 1 is a very fast and competitive play mode. It's purley player against player and whoever defeates the other player first, wins this game.

#### Endless
The endless play mode has no real end goal and is solely for players who like to build an empire. But of course every empire needs to fend off enemies from time to time who want to conquer new territories.

#### Many vs NPC
This play mode is focused on "PvE" meaning that there is only one enemy controlled by a computer.

#### Faction vs Faction
In this mode you have to choose one of two factions before entering the world. The faction who completes an end goal first is going to win.


### World modifier
Each world has a modifier which makes it a bit more individualistic.

#### Speed
Speed modifies
 - Build speed (building, unit, ...)
 - Movement speed (unit)
 - Gathering / Production (ressources)


## MMO
The game shall be designed to be a multiplayer game - with and / or against other players.

### Cooperation
Playing with other players makes it possible to achieve more than playing alone. Objectives and playing versus other players is easier when working together. The game needs to make it easy to coop with other players.

#### Communication
Playing with other players requires ways to communicate with them.

##### Messages
An easy and straight forward way to communicate is to send ingame messages. This should be like sending a letter, required is
 - To = Player name / identifier
 - From = Player name / identifier
 - Subject = Freely choosable
 - Message = Free text (limited to 3000 characters)


#### Groups / Guilds
When playing together it should be easy and also possible to share information.

##### Information of other members
When deciding to play with other members in a guild information should be available of other members. This way it's possible to help and support other members or have common objectives.

###### Incoming attacks
It should be possible to see incoming attacks from other guild members. Having this information it enables members to send defensive units to the member who is getting attacked.

###### Outgoing attacks
When members are attacking other players it should be possible to see those of guild members.


##### Bonuses
There should be no special bonuses available which favor being in a guild or playing as a solo player.


## Combat
At a certain point in time there is going to be combat required to reach the end game or because of other players. The combat system is determenestic and results in the same outcome if all inputs are the same.

In every combat there is an attack and defense force which clash against each other.

### Result calculation
Defence force - Attack force = Remaining defence force + Remaining attack force

### Loot
If attack units survived they will carry back loot. Each unit has their specific loot capacitiy (how much loot it can carry) and loot types (what loot it can carry).

### Combat modes
There are multiple modes for combat available which depend and allow for different play styles. All have in common that following criterias have to be met:
 - To = Players location
 - From = Players location
 - Whom = Number of units
 - Mode = Combat mode

#### Scout
A scout mode is only available when a specific unit is choosen for an attack. The scout reveals information of another players forces and ressources. If the scouted players defense is too high the scout attempt can fail.

#### Raid
The raid mode is for looting as much as possible without engaging in a full-out fight. Units can be lost but usually some survive to carry home loot if the defense force is not too strong. In a raid 
 - only units with a minimum amount of speed can particpate, meaning that slow units are not able to join.
 - only looting is available, no other actions. 

#### War
A war is a full-out engage of forces until one force is completly wiped out. There will be no surivors of the loosing army. In a war, all types of units and actions are available.


# Business requirements

## Races
There should be multiple races avaiable for users to choose from. Each race has a differnt playstyle which makes it unique in it's own way.
The different playstyles have own strengths and weaknesses which result in e.g. different buildings, different units, etc.

### Huns

### Rome

### Germans

### Japanese



## Ressources
Ressources are the EVERYTHING to progress further in Eperica. Units and buildings require ressources to build and upgrade.
There the same ressource types for every player and race available. Ressources can be acquired by respective building.

Ressources are acquired over time and each upgrade enhances the collected ressources over time.

### Wood
The general construction material required to build, train or upgrade.

### Food
When building or training, the village grows and thus requires more and more food.

### Gold
Gold is the currency in Eperica - every building, unit or upgrade requires an amount of gold.

### Iron
Iron is required for higher level buildings or training units.



## Village
A player starts with a given village at a random location.
Each village is on it's own, a village can be built up and grow. A village contains buildings slots which can be used to build buildings. A player makes their own decision in which buildings should be built - there is not a necessity to have certain buildings in a village.

Some villages are better then others in regards to their ressource availability. This means that in general the same amount of ressource buildings slots are availble in every village but the number of slots per ressource can vary.

The starting village has always the same number of buildings slots available to make it fair for every player.


## Buildings
Buildings are used to progress and get new functionality in Eperica. Each building has their own necessesity and allows for a new function to be available. Buildings can be categorized as military, ressources and general but some also are cross functional.

### Military
Military buildings are used for fighting - in an offensive or defensive way.

#### Barracks
In barracks, units on foot can be trained.

#### Stable
In stables, mounted units can be trained. In general, stable units cost more but are also faster and cover more ground in the same time frame as units on foot.

#### Workshop
In a workshop, units for siege can be produced. Siege units are used to destroy buildings in an attack and weaken other players defenses and also their level of buildings so that they have to rebuild them.

#### Forge
A forge is used to upgrade military units.

#### Pallisade
A pallisade is a defensive buildings to enhance your defensive capabilities of units.
Siege units can be used to damage and destroy a pallisade.


### Ressources
Buildings are required to produce ressources.

#### Lumberjack

#### Wheat farm

#### Gold mine

#### Quarry

#### Warehouse

#### Market




### Race specific


## Units

### Race specific



## User groups and access rights

## User stories




# Technical requirements

## Data model