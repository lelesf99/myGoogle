import mmap
from flask import Flask, request, jsonify, send_from_directory
import os
import threading
from models import db, Document
from flask_cors import CORS

app = Flask(__name__)
app.config['SQLALCHEMY_DATABASE_URI'] = 'sqlite:///mygoogle.db'
app.config['SQLALCHEMY_TRACK_MODIFICATIONS'] = False
app.config['UPLOAD_FOLDER'] = 'uploaded_files'
app.config['MAX_CONTENT_LENGTH'] = 8 * 1000 * 1000 * 1000  # 16 Gigabyte limit

db.init_app(app)
CORS(app)


# In-memory storage for tracking file chunks
file_chunks = {}
assembly_locks = {}

@app.route('/list', methods=['GET'])
def list_files():
    files = Document.query.all()
    return jsonify([file.serialize() for file in files])

@app.route('/search', methods=['GET'])
def search_files():
    search_query = request.args.get('query')
    if(search_query == None or search_query == ""):
        return jsonify([])
    files = Document.query.all()
    
    # Open each file and search for the query
    search_results = []
    for file_db in files:
        if(os.path.isfile(file_db.path)):
            with open(os.path.realpath(file_db.path), 'rb', 0) as file_bytes:
                s = mmap.mmap(file_bytes.fileno(), 0, access=mmap.ACCESS_READ)
                # convert search_query to bytes
                search_bytes = search_query.encode('utf-8')
                # search for every occurence of search_query in the file
                index = s.find(search_bytes)
                if(index != -1):
                    result = {"fileName": file_db.name, "file_dbPath": file_db.path, "occurences": []}        
                    while index != -1:
                        # save string near occurence
                        result["occurences"].append({
                            "start": index,
                            "end": index + len(search_bytes),
                            "context": s[index - 20:index + 20].decode('utf-8', errors='ignore')
                        })
                        index = s.find(search_bytes, index + 1)
                    search_results.append(result)
        else:
            print(f"File {file_db.name} not found at {file_db.path}")
            db.session.delete(file_db)
            db.session.commit()
                
    return jsonify(search_results)

# download file
@app.route('/uploaded_files/<path:file_name>', methods=['GET'])
def download_file(file_name):
    file = Document.query.filter_by(name=file_name).first()
    if file:
        uploads = os.path.join(app.root_path, app.config['UPLOAD_FOLDER'])
        return send_from_directory(directory=uploads, path=file_name)
    return jsonify({'message': 'File not found'})

# delete
@app.route('/delete', methods=['DELETE'])
def delete_file():
    file_name = request.args.get('fileName')
    file = Document.query.filter_by(name=file_name).first()
    if file:
        db.session.delete(file)
        db.session.commit()
        os.remove(file.path)
        return jsonify({'message': 'File deleted'})
    return jsonify({'message': 'File not found'})

@app.route('/upload_chunk', methods=['POST'])
def upload_chunk():
    chunk = request.files['chunk']
    chunk_number = int(request.form['chunkNumber'])
    file_name = request.form['fileName']
    folder_name = file_name.split('.')[0]
    total_chunks = int(request.form['totalChunks'])

    # Ensure the directory exists
    file_dir = os.path.join(app.config['UPLOAD_FOLDER'], folder_name)
    if not os.path.exists(file_dir):
        os.makedirs(file_dir, mode=0o755, exist_ok=True)

    # Save chunk to disk
    chunk_path = os.path.join(file_dir, f"{chunk_number:04d}.part")
    with open(chunk_path, 'wb') as f:
        f.write(chunk.read())

    if file_name not in file_chunks:
        file_chunks[file_name] = set()
    file_chunks[file_name].add(chunk_number)
    
    if len(file_chunks[file_name]) == total_chunks:
        # Assemble the file from chunks
        threading.Thread(target=assemble_file, args=(file_name, total_chunks, file_dir)).start()
        file_chunks[file_name] = set()

    return jsonify({'message': f'Chunk {chunk_number} received'})

def assemble_file(file_name, total_chunks, directory):
    target_file_path = os.path.join(app.config['UPLOAD_FOLDER'], file_name)
    with open(target_file_path, 'wb') as target_file:
        for i in range(1, total_chunks + 1):
            part_path = os.path.join(directory, f"{i:04d}.part")
            with open(part_path, 'rb') as part_file:
                target_file.write(part_file.read())
            os.remove(part_path)  # Optional: Remove part file after assembly
        os.rmdir(directory)  # Remove the directory after assembly

    print(f'File {file_name} has been assembled from {total_chunks} parts.')
    add_file_db(file_name, target_file_path)
        
def add_file_db(file_name, file_path):
    app.app_context().push()
    if(Document.query.filter_by(name=file_name).first()):
        return
    new_file = Document(name=file_name, path=file_path)
    db.session.add(new_file)
    db.session.commit()


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5000)
