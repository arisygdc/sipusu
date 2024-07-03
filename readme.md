# SIPUSU (Learning Project)

### under development

**SIPUSU** is a publish-subscribe (pub/sub) application designed **expect** to handle secure and efficient disk I/O operations. The name "SIPUSU" comes from the frequent use of the suffix "sy" in the creator's name, which when pronounced in Indonesian sounds like "si", making it a playful reference to "the pub/sub".

## Features

- Publish-Subscribe Mechanism: Implement a robust pub/sub system that ensures messages are reliably delivered from publishers to subscribers. 
- Disk I/O Optimization: Efficiently handle disk I/O operations to ensure high performance and reliability.
- Custom Secure Protocol: Develop a custom protocol to ensure secure communication between publishers and subscribers.

## Technical Overview
### Publish-Subscribe System

SIPUSU uses a centralized pub/sub system where publishers send messages to specific topics, and subscribers receive messages from these topics. This ensures decoupling between message producers and consumers, allowing for scalable and maintainable code.

### Disk I/O Handling

To explore and optimize disk I/O, SIPUSU uses techniques such as Write-Ahead Logging (WAL) to ensure data integrity and recoverability. This approach ensures that all messages are logged before being processed, allowing for system recovery in case of failure.

### Secure Protocol

SIPUSU aims to implement a custom secure protocol using TLS to encrypt and authenticate messages between publishers and subscribers. This ensures that data is transmitted securely and can only be accessed by authorized parties.

## Roadmap

- [x] Develop a custom secure protocol using TLS for encrypted communication
- [x] worker pool
- [ ] cleanup, unused connection and prefix tree
- [ ] topic shared and not shared
- [ ] Save client state on disk
- [ ] Implement Write-Ahead Logging for message durability
- [ ] Optimize disk I/O handling for high performance

## Contributing

Contributions are welcome! Please fork the repository and submit pull requests.