# VideoHash Indexer
A high-performance service for indexing and searching video hashes using Multi-Index Hashing (MIH).

## Features
- Index and search video hashes with configurable hamming distance
- RESTful API for adding, searching, and deleting hashes

## Getting Started

### Prerequisites

- Rust 1.56 or later
- Cargo

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/videohash_indexer.git
cd videohash_indexer

# Build the project
cargo build --release
```

## Running the Service

```bash
# Start the server
cargo run --release
```

The server will start on http://0.0.0.0:8080 by default.

## API Documentation

### Add/Search for a Hash

```
POST /search
```

Request body:
```json
{
  "video_id": "video-001",
  "hash": "0000000000000000000000000000000000000000000000000000000000000000"
}
```

Response (when no similar hash is found):
```json
{
  "match_found": false,
  "match_details": null,
  "hash_added": true
}
```

Response (when a similar hash is found):
```json
{
  "match_found": true,
  "match_details": {
    "video_id": "video-001",
    "similarity_percentage": 96.875,
    "is_duplicate": true
  },
  "hash_added": false
}
```

### Delete a Hash

```
DELETE /hash/{video_id}
```

Response (success):
```json
{
  "success": true,
  "message": "Hash with video_id video-001 successfully deleted"
}
```

Response (not found):
```json
{
  "error": "Hash with video_id video-001 not found"
}
```

## Running Tests

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run specific test
cargo test test_binary_string_to_u64

# Run with output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Run all integration tests
cargo test --test integration_tests

# Run with verbose output
RUST_LOG=debug cargo test --test integration_tests -- --nocapture
```

### Example Client

The repository includes an example client that demonstrates how to use the API:

```bash
# Make sure the server is running first
cargo run &

# Then run the example client
cargo run --example test_client
```

The example client:
1. Adds a hash for "video-001"
2. Searches for a similar hash for "video-002"
3. Deletes the hash for "video-001"

## Load Testing

A load testing script is included to benchmark the service:

```bash
# Install wrk if you don't have it
# On Ubuntu: sudo apt install wrk
# On macOS: brew install wrk

# Run the load test (12 threads, 400 connections, 30 seconds)
wrk -t12 -c400 -d30s -s src/search_test.lua http://localhost:8080/search
```

## Implementation Details

### Hash Format

The service expects 64-bit binary hashes represented as strings of '0's and '1's. For example:
```
"0000000000000000000000000000000000000000000000000000000000000000"
```

### Similarity Calculation

Similarity is calculated using the Hamming distance between two hashes:
```
similarity_percentage = 100.0 * (64.0 - hamming_distance) / 64.0
```

### Multi-Index Hashing

The service uses the [mih-rs](https://github.com/kampersanda/mih-rs) library for efficient similarity search. The implementation divides the 64-bit hash into 8 blocks of 8 bits each for optimal search performance.

## Development

### Project Structure

```
videohash_indexer/
├── src/
│   ├── main.rs         # Server implementation
│   ├── lib.rs          # Library exports
│   ├── index.rs        # Hash indexing implementation
│   ├── videohash.rs    # Hash validation and parsing
│   ├── examples/
│   │   └── test_client.rs  # Example client
│   └── search_test.lua # Load testing script
├── tests/
│   └── integration_tests.rs  # Integration tests
└── Cargo.toml
```

### Adding New Features

To add new features:

1. Implement the feature in the appropriate module
2. Add unit tests in the module's `tests` submodule
3. Add integration tests in `tests/integration_tests.rs`
4. Update the API documentation in this README

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
